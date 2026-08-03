package handlers

import (
	"context"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base32"
	"encoding/base64"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"image/png"
	"math/big"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/Benwellonedge28/Vimbai/identity-service/database"
	"github.com/Benwellonedge28/Vimbai/identity-service/models"
	"github.com/Benwellonedge28/Vimbai/identity-service/utils"
	"github.com/neo4j/neo4j-go-driver/v4/neo4j"
	"golang.org/x/crypto/bcrypt"
)

// MFA Configuration
const (
	// TOTP parameters
	TOTPDigits     = 6
	TOTPPeriod     = 30
	TOTPSkew       = 1 // Allow 1 period before/after for clock drift
)

// MFAStore in-memory store (use Redis in production)
var mfaStore = &MFAStore{
	sessions:    make(map[string]*MFAEnrollmentSession),
	attempts:    make(map[string]*AttemptTracker),
}

type MFAStore struct {
	sessions map[string]*MFAEnrollmentSession
	attempts map[string]*AttemptTracker
}

type MFAEnrollmentSession struct {
	UserID        string
	Secret        string
	Method        string // "totp" or "sms"
	PhoneNumber   string
	BackupCodes   []string
	CreatedAt     time.Time
	ExpiresAt     time.Time
}

type AttemptTracker struct {
	Count     int
	FirstAt   time.Time
	LockUntil time.Time
}

// GenerateMFASetup generates MFA enrollment data for a user
func GenerateMFASetup(w http.ResponseWriter, r *http.Request) {
	userID := r.Context().Value("user_id").(string)

	// Generate a new TOTP secret
	secret, err := generateTOTPSecret(20)
	if err != nil {
		http.Error(w, "Failed to generate secret", http.StatusInternalServerError)
		return
	}

	// Generate backup codes
	backupCodes := generateBackupCodes(10)

	// Store enrollment session temporarily
	sessionToken := generateSecureToken(32)
	mfaStore.sessions[sessionToken] = &MFAEnrollmentSession{
		UserID:      userID,
		Secret:      secret,
		Method:      "totp",
		BackupCodes: backupCodes,
		CreatedAt:   time.Now(),
		ExpiresAt:   time.Now().Add(10 * time.Minute),
	}

	// Generate OTPAuth URL for QR code
	issuer := getEnv("OIDC_ISSUER", "Vimbai")
	otpAuthURL := fmt.Sprintf("otpauth://totp/%s:%s?secret=%s&issuer=%s&algorithm=SHA1&digits=%d&period=%d",
		issuer, userID, secret, issuer, TOTPDigits, TOTPPeriod)

	// Return setup data
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"session_token":   sessionToken,
		"secret":          secret,
		"otp_auth_url":    otpAuthURL,
		"backup_codes":    backupCodes,
		"expires_in":      600, // 10 minutes
	})
}

// VerifyMFASetup verifies the TOTP code and completes MFA enrollment
func VerifyMFASetup(w http.ResponseWriter, r *http.Request) {
	var req struct {
		SessionToken string `json:"session_token"`
		Code         string `json:"code"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "Invalid request", http.StatusBadRequest)
		return
	}

	session, exists := mfaStore.sessions[req.SessionToken]
	if !exists || time.Now().After(session.ExpiresAt) {
		http.Error(w, "Invalid or expired session", http.StatusBadRequest)
		return
	}

	// Verify TOTP code
	if !validateTOTP(session.Secret, req.Code) {
		http.Error(w, "Invalid verification code", http.StatusBadRequest)
		return
	}

	userID := session.UserID

	// Save MFA data to database
	session := database.Driver.NewSession(neo4j.SessionConfig{AccessMode: neo4j.AccessModeWrite})
	defer session.Close()

	_, err := session.WriteTransaction(func(tx neo4j.Transaction) (interface{}, error) {
		query := `
			MATCH (u:User {id: $userID})
			SET u.mfa_enabled = true,
				u.mfa_secret = $secret,
				u.mfa_method = 'totp',
				u.mfa_backup_codes = $backupCodes,
				u.mfa_enrolled_at = datetime(),
				u.updated_at = datetime()
			RETURN u.id
		`
		_, err := tx.Run(query, map[string]interface{}{
			"userID":       userID,
			"secret":       session.Secret,
			"backupCodes":  strings.Join(session.BackupCodes, ","),
		})
		return nil, err
	})

	if err != nil {
		http.Error(w, "Failed to save MFA data", http.StatusInternalServerError)
		return
	}

	// Clean up enrollment session
	delete(mfaStore.sessions, req.SessionToken)

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]string{
		"message": "MFA enabled successfully",
	})
}

// VerifyMFA verifies MFA code during login
func VerifyMFA(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Username string `json:"username"`
		Password string `json:"password"`
		Code     string `json:"code"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "Invalid request", http.StatusBadRequest)
		return
	}

	// Check for brute force
	if mfaStore.attempts[req.Username] != nil {
		tracker := mfaStore.attempts[req.Username]
		if time.Now().Before(tracker.LockUntil) {
			http.Error(w, "Account temporarily locked due to too many failed attempts", http.StatusTooManyRequests)
			return
		}
	}

	// Get user from database
	dbSession := database.Driver.NewSession(neo4j.SessionConfig{AccessMode: neo4j.AccessModeRead})
	userData, err := dbSession.ReadTransaction(func(tx neo4j.Transaction) (interface{}, error) {
		query := `
			MATCH (u:User {username: $username})-[:HAS_ROLE]->(r:Role)
			RETURN u.id AS id, u.password_hash AS password_hash, u.mfa_enabled AS mfa_enabled,
			       u.mfa_secret AS mfa_secret, u.mfa_backup_codes AS mfa_backup_codes,
			       r.name AS roleName, r.permissions AS permissions
		`
		result, err := tx.Run(query, map[string]interface{}{"username": req.Username})
		if err != nil {
			return nil, err
		}
		if !result.Next() {
			return nil, fmt.Errorf("user not found")
		}
		record := result.Record()
		permissions := []string{}
		if perms := record.GetByIndex(6); perms != nil {
			for _, p := range perms.([]interface{}) {
				permissions = append(permissions, p.(string))
			}
		}
		return map[string]interface{}{
			"id":             record.GetByIndex(0),
			"password_hash":  record.GetByIndex(1),
			"mfa_enabled":    record.GetByIndex(2),
			"mfa_secret":     record.GetByIndex(3),
			"mfa_backup_codes": record.GetByIndex(4),
			"roleName":       record.GetByIndex(5),
			"permissions":    permissions,
		}, nil
	})
	dbSession.Close()

	if err != nil {
		trackFailedAttempt(req.Username)
		http.Error(w, "Invalid credentials", http.StatusUnauthorized)
		return
	}

	userMap := userData.(map[string]interface{})

	// Verify password
	if err := bcrypt.CompareHashAndPassword([]byte(userMap["password_hash"].(string)), []byte(req.Password)); err != nil {
		trackFailedAttempt(req.Username)
		http.Error(w, "Invalid credentials", http.StatusUnauthorized)
		return
	}

	// Verify MFA if enabled
	if userMap["mfa_enabled"] == true {
		mfaSecret := userMap["mfa_secret"].(string)
		backupCodes := strings.Split(userMap["mfa_backup_codes"].(string), ",")

		// Check if code is a backup code
		isBackupCode := false
		for i, code := range backupCodes {
			if code == req.Code && code != "" {
				// Invalidate used backup code
				backupCodes[i] = ""
				isBackupCode = true
				break
			}
		}

		if !isBackupCode && !validateTOTP(mfaSecret, req.Code) {
			trackFailedAttempt(req.Username)
			http.Error(w, "Invalid MFA code", http.StatusUnauthorized)
			return
		}

		// Update backup codes if one was used
		if isBackupCode {
			updateBackupCodes(userMap["id"].(string), backupCodes)
		}
	}

	// Reset failed attempts on success
	delete(mfaStore.attempts, req.Username)

	// Generate JWT
	token, err := utils.GenerateJWT(
		userMap["id"].(string),
		req.Username,
		userMap["roleName"].(string),
		userMap["permissions"].([]string),
	)
	if err != nil {
		http.Error(w, "Failed to generate token", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"token":       token,
		"mfa_verified": userMap["mfa_enabled"] == true,
		"expires_in":   86400,
	})
}

// DisableMFA disables MFA for a user
func DisableMFA(w http.ResponseWriter, r *http.Request) {
	userID := r.Context().Value("user_id").(string)

	var req struct {
		Password string `json:"password"`
		Code     string `json:"code"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "Invalid request", http.StatusBadRequest)
		return
	}

	// Verify password and MFA code
	dbSession := database.Driver.NewSession(neo4j.SessionConfig{AccessMode: neo4j.AccessModeRead})
	userData, err := dbSession.ReadTransaction(func(tx neo4j.Transaction) (interface{}, error) {
		query := `
			MATCH (u:User {id: $userID})
			RETURN u.password_hash AS password_hash, u.mfa_secret AS mfa_secret
		`
		result, err := tx.Run(query, map[string]interface{}{"userID": userID})
		if err != nil {
			return nil, err
		}
		if !result.Next() {
			return nil, fmt.Errorf("user not found")
		}
		record := result.Record()
		return map[string]interface{}{
			"password_hash": record.GetByIndex(0),
			"mfa_secret":    record.GetByIndex(1),
		}, nil
	})
	dbSession.Close()

	if err != nil {
		http.Error(w, "User not found", http.StatusNotFound)
		return
	}

	userMap := userData.(map[string]interface{})

	// Verify password
	if err := bcrypt.CompareHashAndPassword([]byte(userMap["password_hash"].(string)), []byte(req.Password)); err != nil {
		http.Error(w, "Invalid password", http.StatusUnauthorized)
		return
	}

	// Verify MFA code
	if !validateTOTP(userMap["mfa_secret"].(string), req.Code) {
		http.Error(w, "Invalid MFA code", http.StatusUnauthorized)
		return
	}

	// Disable MFA
	writeSession := database.Driver.NewSession(neo4j.SessionConfig{AccessMode: neo4j.AccessModeWrite})
	_, err = writeSession.WriteTransaction(func(tx neo4j.Transaction) (interface{}, error) {
		query := `
			MATCH (u:User {id: $userID})
			SET u.mfa_enabled = false,
				u.mfa_secret = null,
				u.mfa_backup_codes = null,
				u.updated_at = datetime()
		`
		_, err := tx.Run(query, map[string]interface{}{"userID": userID})
		return nil, err
	})
	writeSession.Close()

	if err != nil {
		http.Error(w, "Failed to disable MFA", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]string{
		"message": "MFA disabled successfully",
	})
}

// GetMFAStatus returns MFA status for the current user
func GetMFAStatus(w http.ResponseWriter, r *http.Request) {
	userID := r.Context().Value("user_id").(string)

	session := database.Driver.NewSession(neo4j.SessionConfig{AccessMode: neo4j.AccessModeRead})
	defer session.Close()

	result, err := session.ReadTransaction(func(tx neo4j.Transaction) (interface{}, error) {
		query := `
			MATCH (u:User {id: $userID})
			RETURN u.mfa_enabled AS enabled, u.mfa_method AS method, u.mfa_enrolled_at AS enrolled_at
		`
		result, err := tx.Run(query, map[string]interface{}{"userID": userID})
		if err != nil {
			return nil, err
		}
		if !result.Next() {
			return nil, fmt.Errorf("user not found")
		}
		record := result.Record()
		return map[string]interface{}{
			"enabled":     record.GetByIndex(0),
			"method":      record.GetByIndex(1),
			"enrolled_at": record.GetByIndex(2),
		}, nil
	})

	if err != nil {
		http.Error(w, "Failed to get MFA status", http.StatusInternalServerError)
		return
	}

	data := result.(map[string]interface{})
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"enabled":     data["enabled"] == true,
		"method":      data["method"],
		"enrolled_at": data["enrolled_at"],
	})
}

// TOTP Functions

func generateTOTPSecret(length int) (string, error) {
	bytes := make([]byte, length)
	if _, err := rand.Read(bytes); err != nil {
		return "", err
	}
	return base32.StdEncoding.EncodeToString(bytes), nil
}

func validateTOTP(secret, code string) bool {
	// Decode the secret
	secretBytes, err := base32.StdEncoding.DecodeString(strings.ToUpper(secret))
	if err != nil {
		return false
	}

	// Get current time and check adjacent periods for clock drift
	now := time.Now().Unix()
	for offset := -TOTPSkew; offset <= TOTPSkew; offset++ {
		if generateTOTP(secretBytes, now+int64(offset*TOTPPeriod)) == code {
			return true
		}
	}
	return false
}

func generateTOTP(secret []byte, counter int64) string {
	// Convert counter to bytes (big-endian)
	msg := make([]byte, 8)
	binary.BigEndian.PutUint64(msg, uint64(counter))

	// HMAC-SHA1
	h := hmac.New(sha256.New, secret)
	h.Write(msg)
	hash := h.Sum(nil)

	// Dynamic truncation
	offset := hash[len(hash)-1] & 0x0f
	truncated := binary.BigEndian.Uint32(hash[offset:offset+4]) & 0x7fffffff

	// Get last 6 digits
	code := truncated % 1000000
	return fmt.Sprintf("%06d", code)
}

func generateBackupCodes(count int) []string {
	codes := make([]string, count)
	for i := 0; i < count; i++ {
		bytes := make([]byte, 8)
		rand.Read(bytes)
		codes[i] = fmt.Sprintf("%08x-%04x", binary.BigEndian.Uint64(bytes[:8]), i)
	}
	return codes
}

func trackFailedAttempt(username string) {
	if mfaStore.attempts[username] == nil {
		mfaStore.attempts[username] = &AttemptTracker{}
	}
	tracker := mfaStore.attempts[username]
	tracker.Count++
	if tracker.FirstAt.IsZero() {
		tracker.FirstAt = time.Now()
	}

	// Lock after 5 failed attempts for 15 minutes
	if tracker.Count >= 5 {
		tracker.LockUntil = time.Now().Add(15 * time.Minute)
		tracker.Count = 0
	}
}

func updateBackupCodes(userID string, codes []string) {
	session := database.Driver.NewSession(neo4j.SessionConfig{AccessMode: neo4j.AccessModeWrite})
	defer session.Close()

	session.WriteTransaction(func(tx neo4j.Transaction) (interface{}, error) {
		query := `
			MATCH (u:User {id: $userID})
			SET u.mfa_backup_codes = $backupCodes
		`
		_, err := tx.Run(query, map[string]interface{}{
			"userID":       userID,
			"backupCodes":  strings.Join(codes, ","),
		})
		return nil, err
	})
}

func generateQRCode(otpAuthURL string) ([]byte, error) {
	// In production, use a QR code library like github.com/skip2/go-qrcode
	// For now, return a placeholder
	return []byte{}, nil
}