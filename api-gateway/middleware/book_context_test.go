package middleware

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/labstack/echo/v4"
)

func TestBookContextNoHeaderPassesThrough(t *testing.T) {
	e := echo.New()
	mw := BookContextMiddleware(BookContextConfig{
		BookSyncURL: "http://127.0.0.1:1", // unreachable on purpose
		Timeout:     time.Second,
	})
	called := false
	e.Use(func(next echo.HandlerFunc) echo.HandlerFunc {
		return func(c echo.Context) error {
			called = true
			return next(c)
		}
	})
	e.GET("/x", func(c echo.Context) error {
		return c.String(http.StatusOK, "ok")
	}, mw)
	req := httptest.NewRequest(http.MethodGet, "/x", nil)
	req.Header.Set("X-User-ID", "u1")
	rec := httptest.NewRecorder()
	e.ServeHTTP(rec, req)
	if !called || rec.Code != http.StatusOK {
		t.Fatalf("request without X-Book-ID must pass through, called=%v code=%d", called, rec.Code)
	}
}

func TestBookContextValidatesMembership(t *testing.T) {
	syncSrv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("X-User-ID") != "u1" {
			t.Errorf("membership check must forward X-User-ID, got %q", r.Header.Get("X-User-ID"))
		}
		if r.URL.Path != "/books/b1/membership" {
			t.Errorf("unexpected path %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]string{"role": "bookkeeper", "tier": "business"})
	}))
	defer syncSrv.Close()

	e := echo.New()
	mw := BookContextMiddleware(BookContextConfig{BookSyncURL: syncSrv.URL, Timeout: time.Second})
	e.GET("/x", func(c echo.Context) error {
		if c.Request().Header.Get("X-Book-Role") != "bookkeeper" {
			t.Errorf("X-Book-Role not injected, got %q", c.Request().Header.Get("X-Book-Role"))
		}
		if c.Request().Header.Get("X-Book-Tier") != "business" {
			t.Errorf("X-Book-Tier not injected, got %q", c.Request().Header.Get("X-Book-Tier"))
		}
		return c.String(http.StatusOK, "ok")
	}, mw)

	req := httptest.NewRequest(http.MethodGet, "/x", nil)
	req.Header.Set("X-User-ID", "u1")
	req.Header.Set("X-Book-ID", "b1")
	rec := httptest.NewRecorder()
	e.ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("member request must pass, got %d", rec.Code)
	}
}

func TestBookContextRejectsNonMember(t *testing.T) {
	syncSrv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusForbidden)
	}))
	defer syncSrv.Close()

	e := echo.New()
	mw := BookContextMiddleware(BookContextConfig{BookSyncURL: syncSrv.URL, Timeout: time.Second})
	e.GET("/x", func(c echo.Context) error {
		return c.String(http.StatusOK, "ok")
	}, mw)
	req := httptest.NewRequest(http.MethodGet, "/x", nil)
	req.Header.Set("X-User-ID", "u1")
	req.Header.Set("X-Book-ID", "b1")
	rec := httptest.NewRecorder()
	e.ServeHTTP(rec, req)
	if rec.Code != http.StatusForbidden {
		t.Fatalf("non-member must get 403, got %d", rec.Code)
	}
}

func TestBookContextCachesMembership(t *testing.T) {
	calls := 0
	syncSrv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls++
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]string{"role": "viewer", "tier": "personal"})
	}))
	defer syncSrv.Close()

	e := echo.New()
	mw := BookContextMiddleware(BookContextConfig{BookSyncURL: syncSrv.URL, Timeout: time.Second})
	e.GET("/x", func(c echo.Context) error {
		return c.String(http.StatusOK, "ok")
	}, mw)
	for i := 0; i < 3; i++ {
		req := httptest.NewRequest(http.MethodGet, "/x", nil)
		req.Header.Set("X-User-ID", "u1")
		req.Header.Set("X-Book-ID", "b1")
		rec := httptest.NewRecorder()
		e.ServeHTTP(rec, req)
		if rec.Code != http.StatusOK {
			t.Fatalf("request %d must pass, got %d", i, rec.Code)
		}
	}
	if calls != 1 {
		t.Fatalf("membership must be cached across requests, calls=%d", calls)
	}
}
