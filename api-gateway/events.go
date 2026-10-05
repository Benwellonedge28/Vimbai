package main

import (
	"bytes"
	"encoding/json"
	"io"
	"log"
	"net/http"
	"strings"
	"time"

	"github.com/labstack/echo/v4"
)

// ============================================================================
// Autonomous Book Event Emission
//
// Every successful write (POST/PUT/PATCH/DELETE) inside a Book context is
// published to the message bus as a Book event. The bus then runs the
// autonomous "afterwards" reactions for the Book: webhook fan-out to
// subscribers and built-in automations such as alert-rule re-evaluation.
//
// Emission is fire-and-forget: it never blocks or fails the client
// response, and any bus outage is logged and skipped.
// ============================================================================

// busEmitTarget is the direct target URL of the message-bus service,
// captured at route-registration time. Empty means emission is disabled
// (no message-bus route configured).
var busEmitTarget string

// writeEventActions maps write methods to the CRUD action of the event.
var writeEventActions = map[string]string{
	http.MethodPost:   "created",
	http.MethodPut:    "updated",
	http.MethodPatch:  "updated",
	http.MethodDelete: "deleted",
}

// bookEventTypeFor maps a proxied write request to the bus event type.
// Returns "" when the request must not emit an event (reads, unknown).
func bookEventTypeFor(method, reqPath string) string {
	action, isWrite := writeEventActions[method]
	if !isWrite {
		return ""
	}
	switch {
	case strings.Contains(reqPath, "/journal"):
		return "accounting.journal_entry." + action
	case strings.Contains(reqPath, "/transaction"):
		return "banking.transaction." + action
	default:
		return "book.resource." + action
	}
}

// maybeEmitBookEvent publishes a Book CRUD event after a successful
// proxied write. All request data is copied synchronously; only the
// outbound HTTP call happens in the background goroutine.
func (prh *ProxyResilienceHandler) maybeEmitBookEvent(c echo.Context, status int) {
	if busEmitTarget == "" || prh.routePath == "/message-bus" || status >= 400 {
		return
	}
	method := c.Request().Method
	reqPath := c.Request().URL.Path
	eventType := bookEventTypeFor(method, reqPath)
	if eventType == "" {
		return
	}

	// Copy identity (gateway-injected) before returning from the handler.
	bookID := c.Request().Header.Get("X-Book-ID")
	if bookID == "" {
		return // not a Book-scoped request
	}
	userID := c.Request().Header.Get("X-User-ID")
	if userID == "" {
		return // no verified actor: never emit anonymous events
	}
	bookRole := c.Request().Header.Get("X-Book-Role")
	authorization := c.Request().Header.Get("Authorization")

	go emitBookEvent(eventType, method, reqPath, prh.routePath, status,
		bookID, userID, bookRole, authorization)
}

// emitBookEvent performs the fire-and-forget publish to the message bus.
func emitBookEvent(eventType, method, reqPath, service string, status int,
	bookID, userID, bookRole, authorization string) {
	defer func() {
		if r := recover(); r != nil {
			log.Printf("[BookEvents] panic during emit: %v", r)
		}
	}()

	payload, err := json.Marshal(map[string]interface{}{
		"book_id":   bookID,
		"user_id":   userID,
		"method":    method,
		"path":      reqPath,
		"service":   service,
		"status":    status,
		"book_role": bookRole,
	})
	if err != nil {
		log.Printf("[BookEvents] payload marshal error: %v", err)
		return
	}

	publishURL := busEmitTarget + "/events/" + eventType + "/publish?source_service=api-gateway&priority=normal"
	req, err := http.NewRequest(http.MethodPost, publishURL, bytes.NewReader(payload))
	if err != nil {
		log.Printf("[BookEvents] request build error: %v", err)
		return
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-User-Id", userID)
	req.Header.Set("X-Book-ID", bookID)
	if bookRole != "" {
		req.Header.Set("X-Book-Role", bookRole)
	}
	if authorization != "" {
		req.Header.Set("Authorization", authorization)
	}

	client := &http.Client{Timeout: 5 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		log.Printf("[BookEvents] publish of %s failed (bus unreachable): %v", eventType, err)
		return
	}
	defer resp.Body.Close()
	_, _ = io.Copy(io.Discard, resp.Body)
	if resp.StatusCode >= 400 {
		log.Printf("[BookEvents] publish of %s rejected by bus: status %d", eventType, resp.StatusCode)
		return
	}
	log.Printf("[BookEvents] published %s for %s %s (book %s)", eventType, method, reqPath, bookID)
}
