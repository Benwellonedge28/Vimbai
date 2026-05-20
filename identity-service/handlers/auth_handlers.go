package handlers

import (
	"context" // NEW
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"time"

	"github.com/Benwellonedge28/FinAcc/identity-service/database" // NEW
	"github.com/Benwellonedge28/FinAcc/identity-service/models"
	"github.com/Benwellonedge28/FinAcc/identity-service/utils"
	"github.com/neo4j/neo4j-go-driver/v4/neo4j" // NEW
	"golang.org/x/crypto/bcrypt"
)

// RegisterUser handles new user registration, saving to Neo4j.
func RegisterUser(w http.ResponseWriter, r *http.Request) {
	var reqUser struct { // Using a struct to capture password for hashing before creating models.User
		Username string `json:"username"`
		Password string `json:"password"`
		Email    string `json:"email"`
		RoleName string `json:"role_name"` // Role name instead of ID
	}
	if err := json.NewDecoder(r.Body).Decode(&reqUser); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	// Hash password
	hashedPassword, err := bcrypt.GenerateFromPassword([]byte(reqUser.Password), bcrypt.DefaultCost)
	if err != nil {
		http.Error(w, "Failed to hash password", http.StatusInternalServerError)
		return
	}

	session := database.Driver.NewSession(neo4j.SessionConfig{AccessMode: neo4j.AccessModeWrite})
	defer session.Close()

	user, err := session.WriteTransaction(func(tx neo4j.Transaction) (interface{}, error) {
		// Try to find the role, default to ACCOUNTANT if not specified or found
		roleToAssign := "ACCOUNTANT"
		if reqUser.RoleName != "" {
			result, err := tx.Run("MATCH (r:Role {name: $name}) RETURN r.id AS id, r.name AS name, r.permissions AS permissions", map[string]interface{}{"name": reqUser.RoleName})
			if err != nil {
				return nil, fmt.Errorf("failed to fetch role: %w", err)
			}
			if result.Next() {
				roleToAssign = reqUser.RoleName
			} else {
				// If requested role not found, default to ACCOUNTANT
				logResult(r, "Requested role '%s' not found, defaulting to ACCOUNTANT", reqUser.RoleName)
			}
		}

		// Create the User node
		query := `
			CREATE (u:User {
				id: $id,
				username: $username,
				password_hash: $password_hash,
				email: $email
			})
			WITH u
			MATCH (r:Role {name: $roleName})
			CREATE (u)-[:HAS_ROLE]->(r)
			RETURN u.id AS id, u.username AS username, u.email AS email, r.name AS roleName, r.permissions AS permissions
		`
		result, err := tx.Run(query, map[string]interface{}{
			"id":            fmt.Sprintf("user-%d", time.Now().UnixNano()),
			"username":      reqUser.Username,
			"password_hash": string(hashedPassword),
			"email":         reqUser.Email,
			"roleName":      roleToAssign,
		})
		if err != nil {
			return nil, fmt.Errorf("failed to create user: %w", err)
		}

		record := result.Next()
		if !record {
			return nil, fmt.Errorf("user creation failed unexpectedly")
		}

		userRecord := result.Record()
		permissions := []string{}
		if userRecord.GetByIndex(4) != nil {
			for _, p := range userRecord.GetByIndex(4).([]interface{}) {
				permissions = append(permissions, p.(string))
			}
		}

		return models.User{
			ID:          userRecord.GetByIndex(0).(string),
			Username:    userRecord.GetByIndex(1).(string),
			Email:       userRecord.GetByIndex(2).(string),
			RoleID:      roleToAssign, // Store role name here for clarity
			Permissions: permissions,
		}, nil
	})
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	createdUser := user.(models.User)
	w.WriteHeader(http.StatusCreated)
	json.NewEncoder(w).Encode(map[string]string{"message": "User registered successfully", "user_id": createdUser.ID})
}

// LoginUser handles user login and JWT generation, fetching user/role from Neo4j.
func LoginUser(w http.ResponseWriter, r *http.Request) {
	var credentials struct {
		Username string `json:"username"`
		Password string `json:"password"`
	}
	if err := json.NewDecoder(r.Body).Decode(&credentials); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	session := database.Driver.NewSession(neo4j.SessionConfig{AccessMode: neo4j.AccessModeRead})
	defer session.Close()

	userAndRole, err := session.ReadTransaction(func(tx neo4j.Transaction) (interface{}, error) {
		query := `
			MATCH (u:User {username: $username})-[:HAS_ROLE]->(r:Role)
			RETURN u.id AS id, u.username AS username, u.password_hash AS password_hash, r.name AS roleName, r.permissions AS permissions
		`
		result, err := tx.Run(query, map[string]interface{}{"username": credentials.Username})
		if err != nil {
			return nil, fmt.Errorf("failed to retrieve user: %w", err)
		}

		if !result.Next() {
			return nil, fmt.Errorf("user not found")
		}

		record := result.Record()
		permissions := []string{}
		if record.GetByIndex(4) != nil {
			for _, p := range record.GetByIndex(4).([]interface{}) {
				permissions = append(permissions, p.(string))
			}
		}

		return map[string]interface{}{
			"id":            record.GetByIndex(0).(string),
			"username":      record.GetByIndex(1).(string),
			"password_hash": record.GetByIndex(2).(string),
			"roleName":      record.GetByIndex(3).(string),
			"permissions":   permissions,
		}, nil
	})
	if err != nil {
		http.Error(w, err.Error(), http.StatusUnauthorized) // Mask specific error for security
		return
	}

	userMap := userAndRole.(map[string]interface{})
	passwordHash := userMap["password_hash"].(string)

	if err := bcrypt.CompareHashAndPassword([]byte(passwordHash), []byte(credentials.Password)); err != nil {
		http.Error(w, "Invalid credentials", http.StatusUnauthorized)
		return
	}

	// Generate JWT token
	token, err := utils.GenerateJWT(
		userMap["id"].(string),
		userMap["username"].(string),
		userMap["roleName"].(string),
		userMap["permissions"].([]string),
	)
	if err != nil {
		http.Error(w, "Failed to generate token", http.StatusInternalServerError)
		return
	}

	json.NewEncoder(w).Encode(map[string]string{"token": token})
}

func logResult(r *http.Request, format string, v ...interface{}) {
	logMessage := fmt.Sprintf(format, v...)
	fmt.Printf("[%s] %s %s - %s\n", time.Now().Format("2006-01-02 15:04:05"), r.Method, r.URL.Path, logMessage)
}
