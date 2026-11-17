package utils

import (
	"testing"
	"time"

	"github.com/golang-jwt/jwt/v5"
)

const testSecret = "a secret only this test knows"

func signed(t *testing.T, claims jwt.MapClaims) string {
	t.Helper()
	token, err := jwt.NewWithClaims(jwt.SigningMethodHS256, claims).SignedString([]byte(testSecret))
	if err != nil {
		t.Fatalf("signing a token: %v", err)
	}
	return token
}

func TestTheEngineSubjectNamesTheCaller(t *testing.T) {
	// The engine writes the person in `sub`, so a token it really issues must name them. Reading only the retired
	// service's own `user_id` claim is how the caller arrived downstream nameless.
	engineToken := signed(t, jwt.MapClaims{
		"sub":         "user-18e8c8c3f60ca945-1",
		"type":        "access",
		"role":        "user",
		"permissions": []string{},
		"exp":         time.Now().Add(time.Hour).Unix(),
		"iat":         time.Now().Unix(),
	})
	claims, err := ValidateToken(engineToken, testSecret)
	if err != nil {
		t.Fatalf("a token the engine issues must be accepted: %v", err)
	}
	if claims.UserID != "user-18e8c8c3f60ca945-1" {
		t.Fatalf("the caller must be read from the subject, got %q", claims.UserID)
	}
}

func TestAnOlderTokenKeepsNamingItsOwnUser(t *testing.T) {
	older := signed(t, jwt.MapClaims{
		"user_id": "user-legacy",
		"exp":     time.Now().Add(time.Hour).Unix(),
	})
	claims, err := ValidateToken(older, testSecret)
	if err != nil {
		t.Fatalf("a token naming its user the older way must still work: %v", err)
	}
	if claims.UserID != "user-legacy" {
		t.Fatalf("the older claim must win over an empty subject, got %q", claims.UserID)
	}
}

func TestATokenThatNamesNobodyIsRefused(t *testing.T) {
	nameless := signed(t, jwt.MapClaims{"exp": time.Now().Add(time.Hour).Unix()})
	if _, err := ValidateToken(nameless, testSecret); err == nil {
		t.Fatal("a token that names nobody must be refused rather than passed on unnamed")
	}
}

func TestARefreshTokenIsNotAnAccessToken(t *testing.T) {
	// A refresh token is worth an hour of the owner's authority at the identity door and nothing else: presenting one to
	// a service must not be accepted in place of an access token.
	refresh := signed(t, jwt.MapClaims{
		"sub":  "user-1",
		"type": "refresh",
		"exp":  time.Now().Add(time.Hour).Unix(),
	})
	if _, err := ValidateToken(refresh, testSecret); err == nil {
		t.Fatal("a refresh token must not be accepted as an access token")
	}
}

func TestAnExpiredTokenIsRefused(t *testing.T) {
	expired := signed(t, jwt.MapClaims{
		"sub": "user-1",
		"exp": time.Now().Add(-time.Minute).Unix(),
	})
	if _, err := ValidateToken(expired, testSecret); err == nil {
		t.Fatal("an expired token must be refused")
	}
}
