package middleware

import (
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/labstack/echo/v4"
)

// BookContextMiddleware wires the shared-Book (audience-tier) context into
// every downstream service. A client that wants to act inside a specific
// Book - personal, household, group, business, nonprofit or an
// organization's Book - sends X-Book-ID. The middleware verifies the caller
// is an ACTIVE member of that Book via the book-sync service, then forwards
// X-Book-ID and X-Book-Role to the upstream so every service can scope and
// authorize data by Book context. Requests without X-Book-ID stay
// user-scoped (personal defaults still work exactly as before).
//
// Must run AFTER AuthMiddleware (it relies on the injected X-User-ID).

const bookMembershipCacheTTL = 60 * time.Second

type bookRole struct {
	role string
	tier string
}

type bookContextCache struct {
	mu    sync.RWMutex
	roles map[string]bookRoleEntry // key: userID + "|" + bookID
}

type bookRoleEntry struct {
	role     bookRole
	deadline time.Time
}

// BookContextConfig configures the middleware.
type BookContextConfig struct {
	// BookSyncURL is the base URL of the book-sync service (e.g.
	// http://localhost:9020). Resolved from the /book-sync route.
	BookSyncURL string
	// Timeout for the membership lookup call.
	Timeout time.Duration
	// Client is the HTTP client used for lookups (injectable for tests).
	Client *http.Client
}

// NewBookContextCache returns an empty membership cache.
func NewBookContextCache() *bookContextCache {
	return &bookContextCache{roles: make(map[string]bookRoleEntry)}
}

func (c *bookContextCache) get(userID, bookID string) (bookRole, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	e, ok := c.roles[userID+"|"+bookID]
	if !ok || time.Now().After(e.deadline) {
		return bookRole{}, false
	}
	return e.role, true
}

func (c *bookContextCache) set(userID, bookID string, r bookRole) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.roles[userID+"|"+bookID] = bookRoleEntry{role: r, deadline: time.Now().Add(bookMembershipCacheTTL)}
}

// BookContextMiddleware validates X-Book-ID membership and injects the Book
// context headers downstream.
func BookContextMiddleware(cfg BookContextConfig) echo.MiddlewareFunc {
	cache := NewBookContextCache()
	client := cfg.Client
	if client == nil {
		client = &http.Client{Timeout: cfg.Timeout}
	}

	return func(next echo.HandlerFunc) echo.HandlerFunc {
		return func(c echo.Context) error {
			bookID := strings.TrimSpace(c.Request().Header.Get("X-Book-ID"))
			if bookID == "" {
				return next(c) // user-scoped request, no Book context
			}
			userID := c.Request().Header.Get("X-User-ID")
			if userID == "" {
				// AuthMiddleware should always set this; guard anyway.
				return echo.NewHTTPError(http.StatusUnauthorized, "Missing user identity")
			}

			if role, ok := cache.get(userID, bookID); ok {
				c.Request().Header.Set("X-Book-Role", role.role)
				c.Request().Header.Set("X-Book-Tier", role.tier)
				return next(c)
			}

			req, err := http.NewRequestWithContext(
				c.Request().Context(), http.MethodGet,
				fmt.Sprintf("%s/books/%s/membership", cfg.BookSyncURL, bookID), nil,
			)
			if err != nil {
				return echo.NewHTTPError(http.StatusInternalServerError, "Book context lookup failed")
			}
			req.Header.Set("X-User-ID", userID)
			resp, err := client.Do(req)
			if err != nil {
				return echo.NewHTTPError(http.StatusBadGateway, "Book service unreachable")
			}
			defer func() { _ = resp.Body.Close() }()

			if resp.StatusCode == http.StatusForbidden || resp.StatusCode == http.StatusNotFound {
				return echo.NewHTTPError(http.StatusForbidden, "Not an active member of this Book")
			}
			if resp.StatusCode != http.StatusOK {
				return echo.NewHTTPError(http.StatusBadGateway, "Book membership service error")
			}

			var body struct {
				Role string `json:"role"`
				Tier string `json:"tier"`
			}
			if err := json.NewDecoder(resp.Body).Decode(&body); err != nil || body.Role == "" {
				return echo.NewHTTPError(http.StatusBadGateway, "Invalid membership response")
			}

			cache.set(userID, bookID, bookRole{role: body.Role, tier: body.Tier})
			c.Request().Header.Set("X-Book-Role", body.Role)
			c.Request().Header.Set("X-Book-Tier", body.Tier)
			return next(c)
		}
	}
}
