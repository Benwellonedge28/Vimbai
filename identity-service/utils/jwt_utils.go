package utils

import (
	"fmt"
	"os"
	"time"

	"github.com/dgrijalva/jwt-go"
)

// Define a secret key for signing the tokens.
// In a real application, this should be a strong, randomly generated key loaded from environment variables or a secure vault.
var jwtSecret = []byte(os.Getenv("JWT_SECRET")) // Must be set via environment variable

// Claims struct to hold custom claims for the JWT.
type Claims struct {
	UserID      string   `json:"user_id"`
	Username    string `json:"username"`
	Role        string   `json:"role"`
	Permissions []string `json:"permissions"`
	jwt.StandardClaims
}

// GenerateJWT creates a new JWT token for a given user.
func GenerateJWT(userID, username, role string, permissions []string) (string, error) {
	expirationTime := time.Now().Add(15 * time.Minute) // Access token valid for 15 minutes
	claims := &Claims{
		UserID:      userID,
		Username:    username,
		Role:        role,
		Permissions: permissions,
		StandardClaims: jwt.StandardClaims{
			ExpiresAt: expirationTime.Unix(),
			IssuedAt:  time.Now().Unix(),
			Issuer:    "vimbai-identity-service",
		},
	}

	token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
	tokenString, err := token.SignedString(jwtSecret)
	if err != nil {
		return "", err
	}

	return tokenString, nil
}

// ValidateJWT parses and validates a JWT token.
func ValidateJWT(tokenString string) (*Claims, error) {
	claims := &Claims{}
	token, err := jwt.ParseWithClaims(tokenString, claims, func(token *jwt.Token) (interface{}, error) {
		return jwtSecret, nil
	})

	if err != nil {
		return nil, err
	}

	if !token.Valid {
		return nil, fmt.Errorf("invalid token")
	}

	return claims, nil
}
