package middleware

import (
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/golang-jwt/jwt/v5"
	"github.com/labstack/echo/v4"
	"vimbai/api-gateway/config"
)

const authTestSecret = "a secret only this test knows"

func authToken(t *testing.T, claims jwt.MapClaims) string {
	t.Helper()
	token, err := jwt.NewWithClaims(jwt.SigningMethodHS256, claims).SignedString([]byte(authTestSecret))
	if err != nil {
		t.Fatalf("signing a token: %v", err)
	}
	return token
}

// A request that acts in a Book arrives from the phone with a token and nothing else: naming the caller is the gateway's
// job. If it does not, the engine refuses the request for a missing user, and the first thing a stranger does in their own
// Book fails.
func TestTheCallerIsNamedDownstreamFromTheToken(t *testing.T) {
	cfg := &config.Config{
		JwtSecret: authTestSecret,
		Routes:    []config.Route{{Path: "/accounting", AuthRequired: true}},
	}
	engineToken := authToken(t, jwt.MapClaims{
		"sub":         "user-18e8c8c3f60ca945-1",
		"type":        "access",
		"role":        "user",
		"permissions": []string{"journal.read"},
		"exp":         time.Now().Add(time.Hour).Unix(),
	})

	e := echo.New()
	var seen http.Header
	e.Use(AuthMiddleware(cfg))
	e.GET("/accounting/accounts", func(c echo.Context) error {
		seen = c.Request().Header.Clone()
		return c.String(http.StatusOK, "ok")
	})

	req := httptest.NewRequest(http.MethodGet, "/accounting/accounts", nil)
	req.Header.Set("Authorization", "Bearer "+engineToken)
	req.Header.Set("X-Book-ID", "book-1") // the phone names its Book, never its user
	rec := httptest.NewRecorder()
	e.ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("a valid token must be let through, got %d", rec.Code)
	}
	if got := seen.Get("X-User-ID"); got != "user-18e8c8c3f60ca945-1" {
		t.Fatalf("the caller must reach the engine named, got %q", got)
	}
	if got := seen.Get("X-User-Permissions"); got != "journal.read" {
		t.Fatalf("the token's permissions must reach the engine, got %q", got)
	}
	if got := seen.Get("X-Book-ID"); got != "book-1" {
		t.Fatalf("the Book the phone named must survive, got %q", got)
	}
}

// Naming yourself without a token is not being signed in.
func TestAClientCannotNameItsOwnUser(t *testing.T) {
	cfg := &config.Config{
		JwtSecret: authTestSecret,
		Routes:    []config.Route{{Path: "/accounting", AuthRequired: true}},
	}
	reached := false
	e := echo.New()
	e.Use(AuthMiddleware(cfg))
	e.GET("/accounting/accounts", func(c echo.Context) error {
		reached = true
		return c.String(http.StatusOK, "ok")
	})

	req := httptest.NewRequest(http.MethodGet, "/accounting/accounts", nil)
	req.Header.Set("X-User-ID", "user-someone-else")
	rec := httptest.NewRecorder()
	e.ServeHTTP(rec, req)

	if reached || rec.Code != http.StatusUnauthorized {
		t.Fatalf("a request naming its own user without a token must be refused, reached=%v code=%d", reached, rec.Code)
	}
}

// A route that needs no token still must not let a caller name itself: the engine believes the header, so one passed
// through unaudited is a way to act as somebody else on the one path that asks for no password.
func TestAnOpenRouteStillDropsTheCallersOwnName(t *testing.T) {
	cfg := &config.Config{
		JwtSecret: authTestSecret,
		Routes:    []config.Route{{Path: "/identity", AuthRequired: false}},
	}
	var seen http.Header
	e := echo.New()
	e.Use(AuthMiddleware(cfg))
	e.GET("/identity/health", func(c echo.Context) error {
		seen = c.Request().Header.Clone()
		return c.String(http.StatusOK, "ok")
	})

	req := httptest.NewRequest(http.MethodGet, "/identity/health", nil)
	req.Header.Set("X-User-ID", "user-someone-else")
	req.Header.Set("X-User-Role", "admin")
	rec := httptest.NewRecorder()
	e.ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("an open route must answer, got %d", rec.Code)
	}
	if got := seen.Get("X-User-ID"); got != "" {
		t.Fatalf("a name the client sent must not reach the engine, got %q", got)
	}
	if got := seen.Get("X-User-Role"); got != "" {
		t.Fatalf("a role the client sent must not reach the engine, got %q", got)
	}
}

// Signing in is how a caller becomes somebody, so the identity door has to answer before there is a token.
func TestTheIdentityDoorAnswersWithoutAToken(t *testing.T) {
	cfg := &config.Config{
		JwtSecret: authTestSecret,
		Routes:    []config.Route{{Path: "/identity", AuthRequired: false}},
	}
	e := echo.New()
	e.Use(AuthMiddleware(cfg))
	e.POST("/identity/users/login", func(c echo.Context) error {
		return c.String(http.StatusOK, "token")
	})

	req := httptest.NewRequest(http.MethodPost, "/identity/users/login", nil)
	rec := httptest.NewRecorder()
	e.ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("the identity door must answer before a token exists, got %d", rec.Code)
	}
}
