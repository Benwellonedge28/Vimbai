package utils

import (
	"errors"
	"time"

	"github.com/golang-jwt/jwt/v5"
)

// Claims defines the JWT claims structure
type Claims struct {
	UserID      string   `json:"user_id"`
	Username    string   `json:"username"`
	Role        string   `json:"role"`
	Permissions []string `json:"permissions"`
	// Type is what the identity door issued this for. An access token may act; a refresh token may only be exchanged.
	Type string `json:"type"`
	jwt.RegisteredClaims
}

// ValidateToken validates the JWT token and returns claims
func ValidateToken(tokenString string, jwtSecret string) (*Claims, error) {
	claims := &Claims{}
	token, err := jwt.ParseWithClaims(tokenString, claims, func(token *jwt.Token) (interface{}, error) {
		return []byte(jwtSecret), nil
	})

	if err != nil {
		return nil, err
	}

	if !token.Valid {
		return nil, errors.New("invalid token")
	}

	// Check expiration
	if claims.ExpiresAt != nil && claims.ExpiresAt.Before(time.Now()) {
		return nil, errors.New("token expired")
	}

	// Who the token is about. The engine writes the person in `sub`, the way JWT defines the subject; the retired identity
	// service wrote it in its own `user_id` claim. Reading only `user_id` left the caller unnamed for every token the engine
	// issues, so the header handed downstream was empty and the engine refused the request for a missing user - which is the
	// seam between the gateway and the engine, not a client problem.
	if claims.UserID == "" {
		claims.UserID = claims.Subject
	}
	if claims.UserID == "" {
		return nil, errors.New("token names no user")
	}
	if claims.Type == "refresh" {
		return nil, errors.New("a refresh token is not an access token")
	}

	return claims, nil
}
