package handlers

import (
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/Benwellonedge28/Vimbai/identity-service/database"
	"github.com/Benwellonedge28/Vimbai/identity-service/models"
	"github.com/Benwellonedge28/Vimbai/identity-service/utils"
	"github.com/go-chi/chi/v5"
	"github.com/neo4j/neo4j-go-driver/v4/neo4j"
	"golang.org/x/crypto/bcrypt"
)

// OAuth2/OIDC Configuration
var (
	oauthProviders = map[string]models.OAuthProvider{
		"google": {
			Provider:     "google",
			ClientID:     getEnv("GOOGLE_CLIENT_ID", ""),
			ClientSecret: getEnv("GOOGLE_CLIENT_SECRET", ""),
			AuthURL:      "https://accounts.google.com/o/oauth2/v2/auth",
			TokenURL:     "https://oauth2.googleapis.com/token",
			UserInfoURL:  "https://www.googleapis.com/oauth2/v2/userinfo",
			Scopes:       []string{"openid", "email", "profile"},
			Enabled:      getEnv("GOOGLE_CLIENT_ID", "") != "",
		},
		"github": {
			Provider:     "github",
			ClientID:     getEnv("GITHUB_CLIENT_ID", ""),
			ClientSecret: getEnv("GITHUB_CLIENT_SECRET", ""),
			AuthURL:      "https://github.com/login/oauth/authorize",
			TokenURL:     "https://github.com/login/oauth/access_token",
			UserInfoURL:  "https://api.github.com/user",
			Scopes:       []string{"user:email"},
			Enabled:      getEnv("GITHUB_CLIENT_ID", "") != "",
		},
		"microsoft": {
			Provider:     "microsoft",
			ClientID:     getEnv("MICROSOFT_CLIENT_ID", ""),
			ClientSecret: getEnv("MICROSOFT_CLIENT_SECRET", ""),
			AuthURL:      "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
			TokenURL:     "https://login.microsoftonline.com/common/oauth2/v2.0/token",
			UserInfoURL:  "https://graph.microsoft.com/v1.0/me",
			Scopes:       []string{"openid", "email", "profile"},
			Enabled:      getEnv("MICROSOFT_CLIENT_ID", "") != "",
		},
	}
)

// OAuthAuthorize initiates OAuth2 authorization flow
func OAuthAuthorize(w http.ResponseWriter, r *http.Request) {
	providerName := chi.URLParam(r, "provider")
	provider, exists := oauthProviders[providerName]

	if !exists || !provider.Enabled {
		http.Error(w, "OAuth provider not configured", http.StatusBadRequest)
		return
	}

	// Generate state for CSRF protection
	state := generateSecureToken(32)

	// Store state in session
	sessionStore.Set(state, models.OAuthSession{Provider: providerName, CreatedAt: time.Now()})

	// Build authorization URL
	authURL, _ := url.Parse(provider.AuthURL)
	query := authURL.Query()
	query.Set("client_id", provider.ClientID)
	query.Set("redirect_uri", getEnv("OAUTH_REDIRECT_URI", "http://localhost:8080/oauth/callback"))
	query.Set("response_type", "code")
	query.Set("scope", strings.Join(provider.Scopes, " "))
	query.Set("state", state)
	authURL.RawQuery = query.Encode()

	http.Redirect(w, r, authURL.String(), http.StatusTemporaryRedirect)
}

// OAuthCallback handles OAuth2 callback
func OAuthCallback(w http.ResponseWriter, r *http.Request) {
	code := r.URL.Query().Get("code")
	state := r.URL.Query().Get("state")
	errorParam := r.URL.Query().Get("error")

	if errorParam != "" {
		http.Error(w, "OAuth error: "+r.URL.Query().Get("error_description"), http.StatusBadRequest)
		return
	}

	// Validate state
	session, exists := sessionStore.Get(state)
	if !exists {
		http.Error(w, "Invalid state parameter", http.StatusBadRequest)
		return
	}
	sessionStore.Delete(state)

	provider := oauthProviders[session.Provider]

	// Exchange code for token
	tokenResp, err := exchangeCode(provider, code)
	if err != nil {
		http.Error(w, "Failed to exchange code: "+err.Error(), http.StatusInternalServerError)
		return
	}

	// Get user info
	userInfo, err := getOAuthUserInfo(provider, tokenResp.AccessToken)
	if err != nil {
		http.Error(w, "Failed to get user info: "+err.Error(), http.StatusInternalServerError)
		return
	}

	// Create or update user
	user, err := upsertOAuthUser(session.Provider, userInfo)
	if err != nil {
		http.Error(w, "Failed to process user: "+err.Error(), http.StatusInternalServerError)
		return
	}

	// Generate JWT
	jwtToken, err := utils.GenerateJWT(
		user.ID,
		user.Username,
		user.RoleID,
		user.Permissions,
	)
	if err != nil {
		http.Error(w, "Failed to generate token", http.StatusInternalServerError)
		return
	}

	// Return token
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"token":      jwtToken,
		"user":       user,
		"provider":   session.Provider,
		"expires_in": tokenResp.ExpiresIn,
	})
}

// ExchangeCode exchanges authorization code for access token
func exchangeCode(provider models.OAuthProvider, code string) (*models.OAuthTokenResponse, error) {
	data := url.Values{}
	data.Set("client_id", provider.ClientID)
	data.Set("client_secret", provider.ClientSecret)
	data.Set("code", code)
	data.Set("redirect_uri", getEnv("OAUTH_REDIRECT_URI", "http://localhost:8080/oauth/callback"))
	data.Set("grant_type", "authorization_code")

	resp, err := http.PostForm(provider.TokenURL, data)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var tokenResp models.OAuthTokenResponse
	if err := json.NewDecoder(resp.Body).Decode(&tokenResp); err != nil {
		return nil, err
	}

	return &tokenResp, nil
}

// GetOAuthUserInfo fetches user information from OAuth provider
func getOAuthUserInfo(provider models.OAuthProvider, accessToken string) (*models.OAuthUserInfo, error) {
	req, _ := http.NewRequest("GET", provider.UserInfoURL, nil)
	req.Header.Set("Authorization", "Bearer "+accessToken)

	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	body, _ := io.ReadAll(resp.Body)

	var userInfo models.OAuthUserInfo
	switch provider.Provider {
	case "google":
		var googleUser struct {
			ID            string `json:"id"`
			Email         string `json:"email"`
			VerifiedEmail bool   `json:"verified_email"`
			Name          string `json:"name"`
			Picture       string `json:"picture"`
		}
		if err := json.Unmarshal(body, &googleUser); err != nil {
			return nil, err
		}
		userInfo = models.OAuthUserInfo{
			ProviderID: googleUser.ID,
			Email:      googleUser.Email,
			Name:       googleUser.Name,
			Picture:    googleUser.Picture,
		}
	case "github":
		var githubUser struct {
			ID    int64  `json:"id"`
			Login string `json:"login"`
			Name  string `json:"name"`
			Email string `json:"email"`
		}
		if err := json.Unmarshal(body, &githubUser); err != nil {
			return nil, err
		}
		userInfo = models.OAuthUserInfo{
			ProviderID: fmt.Sprintf("%d", githubUser.ID),
			Email:      githubUser.Email,
			Name:       githubUser.Name,
			Username:   githubUser.Login,
		}
	}
	return &userInfo, nil
}

// UpsertOAuthUser creates or updates user from OAuth provider
func upsertOAuthUser(provider string, userInfo *models.OAuthUserInfo) (*models.User, error) {
	session := database.Driver.NewSession(neo4j.SessionConfig{AccessMode: neo4j.AccessModeWrite})
	defer session.Close()

	result, err := session.WriteTransaction(func(tx neo4j.Transaction) (interface{}, error) {
		// Check if user with this OAuth provider exists
		query := `
			MATCH (u:User)-[:LINKED_OAUTH]->(op:OAuthProvider {provider: $provider, provider_id: $providerID})
			RETURN u
		`
		res, err := tx.Run(query, map[string]interface{}{
			"provider":   provider,
			"providerID": userInfo.ProviderID,
		})
		if err != nil {
			return nil, err
		}

		if res.Next() {
			// Update existing user
			record := res.Record()
			userNode := record.GetByIndex(0).(neo4j.Node)
			return models.User{
				ID:       userNode.Props["id"].(string),
				Username: userNode.Props["username"].(string),
				Email:    userNode.Props["email"].(string),
				RoleID:   userNode.Props["role_id"].(string),
			}, nil
		}

		// Create new user
		userID := fmt.Sprintf("user-%d", time.Now().UnixNano())
		username := userInfo.Username
		if username == "" {
			username = strings.Split(userInfo.Email, "@")[0]
		}

		createQuery := `
			CREATE (u:User {
				id: $id,
				username: $username,
				email: $email,
				password_hash: $passwordHash,
				is_oauth_user: true,
				email_verified: true,
				created_at: datetime(),
				updated_at: datetime()
			})
			CREATE (op:OAuthProvider {
				provider: $provider,
				provider_id: $providerID,
				linked_at: datetime()
			})
			CREATE (u)-[:LINKED_OAUTH]->(op)
			WITH u
			MATCH (r:Role {name: 'ACCOUNTANT'})
			CREATE (u)-[:HAS_ROLE]->(r)
			RETURN u.id AS id, u.username AS username, u.email AS email, r.name AS roleName, r.permissions AS permissions
		`

		// Generate a random password hash for OAuth users
		randomPassword := generateSecureToken(32)
		hashedPassword, _ := bcrypt.GenerateFromPassword([]byte(randomPassword), bcrypt.DefaultCost)

		res, err = tx.Run(createQuery, map[string]interface{}{
			"id":           userID,
			"username":     username,
			"email":        userInfo.Email,
			"passwordHash": string(hashedPassword),
			"provider":     provider,
			"providerID":   userInfo.ProviderID,
		})
		if err != nil {
			return nil, err
		}

		if res.Next() {
			record := res.Record()
			permissions := []string{}
			if perms := record.GetByIndex(4); perms != nil {
				for _, p := range perms.([]interface{}) {
					permissions = append(permissions, p.(string))
				}
			}
			return models.User{
				ID:          record.GetByIndex(0).(string),
				Username:    record.GetByIndex(1).(string),
				Email:       record.GetByIndex(2).(string),
				RoleID:      record.GetByIndex(3).(string),
				Permissions: permissions,
			}, nil
		}

		return nil, fmt.Errorf("user creation failed")
	})

	if err != nil {
		return nil, err
	}

	user := result.(models.User)
	return &user, nil
}

// GetOAuthProviders returns list of enabled OAuth providers
func GetOAuthProviders(w http.ResponseWriter, r *http.Request) {
	enabledProviders := []models.OAuthProviderInfo{}
	for name, provider := range oauthProviders {
		if provider.Enabled {
			enabledProviders = append(enabledProviders, models.OAuthProviderInfo{
				Name:        name,
				DisplayName: getProviderDisplayName(name),
				Icon:        getProviderIcon(name),
			})
		}
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"providers": enabledProviders,
	})
}

// OIDCDiscovery returns OpenID Connect discovery document
func OIDCDiscovery(w http.ResponseWriter, r *http.Request) {
	issuer := getEnv("OIDC_ISSUER", "http://localhost:8080")

	discovery := models.OIDCDiscovery{
		Issuer:                            issuer,
		AuthorizationEndpoint:             issuer + "/authorize",
		TokenEndpoint:                     issuer + "/oauth/token",
		UserInfoEndpoint:                  issuer + "/oauth/userinfo",
		JwksURI:                           issuer + "/.well-known/jwks.json",
		ResponseTypesSupported:            []string{"code", "token", "id_token", "code token", "code id_token", "token id_token", "code token id_token"},
		SubjectTypesSupported:             []string{"public"},
		IDTokenSigningAlgValuesSupported:  []string{"RS256", "HS256"},
		ScopesSupported:                   []string{"openid", "profile", "email"},
		TokenEndpointAuthMethodsSupported: []string{"client_secret_post", "client_secret_basic"},
		ClaimsSupported: []string{
			"sub", "name", "given_name", "family_name", "preferred_username",
			"email", "email_verified", "picture", "locale", "zoneinfo",
		},
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(discovery)
}

// JWKS returns JSON Web Key Set
func JWKS(w http.ResponseWriter, r *http.Request) {
	jwks := models.JWKS{
		Keys: []models.JWK{
			{
				Kty: "RSA",
				Use: "sig",
				Kid: "vimbai-key-1",
				Alg: "RS256",
				N:   "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw",
				E:   "AQAB",
			},
		},
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(jwks)
}

// Helper functions

func generateSecureToken(length int) string {
	bytes := make([]byte, length)
	rand.Read(bytes)
	return base64.URLEncoding.EncodeToString(bytes)[:length]
}

func getProviderDisplayName(name string) string {
	names := map[string]string{
		"google":    "Google",
		"github":    "GitHub",
		"microsoft": "Microsoft",
	}
	if displayName, ok := names[name]; ok {
		return displayName
	}
	return name
}

func getProviderIcon(name string) string {
	icons := map[string]string{
		"google":    "google-icon",
		"github":    "github-icon",
		"microsoft": "microsoft-icon",
	}
	if icon, ok := icons[name]; ok {
		return icon
	}
	return "default-icon"
}

func getEnv(key, defaultValue string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return defaultValue
}

// Simple in-memory session store (use Redis in production)
type OAuthSessionStore struct {
	sessions map[string]models.OAuthSession
	mu       struct {
		sync.Mutex
	}
}

var sessionStore = &OAuthSessionStore{
	sessions: make(map[string]models.OAuthSession),
}

func (s *OAuthSessionStore) Set(token string, session models.OAuthSession) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.sessions[token] = session
	// Cleanup old sessions
	if len(s.sessions) > 10000 {
		now := time.Now()
		for k, v := range s.sessions {
			if now.Sub(v.CreatedAt) > 10*time.Minute {
				delete(s.sessions, k)
			}
		}
	}
}

func (s *OAuthSessionStore) Get(token string) (models.OAuthSession, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	session, exists := s.sessions[token]
	return session, exists
}

func (s *OAuthSessionStore) Delete(token string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.sessions, token)
}
