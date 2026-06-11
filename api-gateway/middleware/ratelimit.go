package middleware

import (
	"net/http"
	"os"
	"strconv"
	"sync"
	"time"

	"github.com/labstack/echo/v4"
)

// RateLimitConfig holds rate limiting configuration
type RateLimitConfig struct {
	// RequestsPerSecond is the number of requests allowed per second per client
	RequestsPerSecond int
	// BurstSize is the maximum burst size for token bucket
	BurstSize int
	// Enabled controls whether rate limiting is active
	Enabled bool
}

// DefaultRateLimitConfig provides sensible defaults for rate limiting
var DefaultRateLimitConfig = RateLimitConfig{
	RequestsPerSecond: 100,
	BurstSize:        200,
	Enabled:           true,
}

// LoadRateLimitConfig loads rate limit configuration from environment variables
func LoadRateLimitConfig() RateLimitConfig {
	cfg := DefaultRateLimitConfig

	if rps := getEnvInt("RATE_LIMIT_RPS", 100); rps > 0 {
		cfg.RequestsPerSecond = rps
	}

	if burst := getEnvInt("RATE_LIMIT_BURST", 200); burst > 0 {
		cfg.BurstSize = burst
	}

	if enabled := getEnv("RATE_LIMIT_ENABLED", "true"); enabled == "false" {
		cfg.Enabled = false
	}

	return cfg
}

// getEnv retrieves an environment variable or returns a default value
func getEnv(key, defaultValue string) string {
	if value, exists := os.LookupEnv(key); exists {
		return value
	}
	return defaultValue
}

func getEnvInt(key string, defaultVal int) int {
	if val := getEnv(key, ""); val != "" {
		if intVal, err := strconv.Atoi(val); err == nil {
			return intVal
		}
	}
	return defaultVal
}

// TokenBucket implements the token bucket algorithm for rate limiting
type TokenBucket struct {
	tokens         float64
	maxTokens      float64
	refillRate     float64 // tokens per second
	lastRefillTime time.Time
	mu             sync.Mutex
}

// NewTokenBucket creates a new token bucket
func NewTokenBucket(rate int, burst int) *TokenBucket {
	return &TokenBucket{
		tokens:         float64(burst),
		maxTokens:      float64(burst),
		refillRate:     float64(rate),
		lastRefillTime: time.Now(),
	}
}

// Allow checks if a request is allowed and consumes a token if so
func (tb *TokenBucket) Allow() bool {
	tb.mu.Lock()
	defer tb.mu.Unlock()

	// Refill tokens based on elapsed time
	now := time.Now()
	elapsed := now.Sub(tb.lastRefillTime).Seconds()
	tb.tokens += elapsed * tb.refillRate
	if tb.tokens > tb.maxTokens {
		tb.tokens = tb.maxTokens
	}
	tb.lastRefillTime = now

	// Check if we have at least one token
	if tb.tokens >= 1 {
		tb.tokens--
		return true
	}
	return false
}

// GetTokens returns the current number of tokens in the bucket
func (tb *TokenBucket) GetTokens() float64 {
	tb.mu.Lock()
	defer tb.mu.Unlock()

	elapsed := time.Now().Sub(tb.lastRefillTime).Seconds()
	tokens := tb.tokens + elapsed*tb.refillRate
	if tokens > tb.maxTokens {
		tokens = tb.maxTokens
	}
	return tokens
}

// RateLimiter manages rate limiting for multiple clients
type RateLimiter struct {
	buckets map[string]*TokenBucket
	mu      sync.RWMutex
	config  RateLimitConfig
	cleanup *time.Ticker
	done    chan bool
}

// NewRateLimiter creates a new rate limiter
func NewRateLimiter(config RateLimitConfig) *RateLimiter {
	rl := &RateLimiter{
		buckets: make(map[string]*TokenBucket),
		config:  config,
		cleanup: time.NewTicker(5 * time.Minute),
		done:    make(chan bool),
	}

	// Start cleanup goroutine to remove stale buckets
	go func() {
		for {
			select {
			case <-rl.cleanup.C:
				rl.cleanupStaleBuckets()
			case <-rl.done:
				return
			}
		}
	}()

	return rl
}

// cleanupStaleBuckets removes buckets that haven't been used recently
func (rl *RateLimiter) cleanupStaleBuckets() {
	rl.mu.Lock()
	defer rl.mu.Unlock()

	threshold := time.Now().Add(-15 * time.Minute)
	for key, bucket := range rl.buckets {
		bucket.mu.Lock()
		if bucket.lastRefillTime.Before(threshold) {
			delete(rl.buckets, key)
		}
		bucket.mu.Unlock()
	}
}

// Stop stops the rate limiter cleanup goroutine
func (rl *RateLimiter) Stop() {
	close(rl.done)
	rl.cleanup.Stop()
}

// GetBucket returns the token bucket for a given client identifier
func (rl *RateLimiter) GetBucket(clientID string) *TokenBucket {
	rl.mu.RLock()
	bucket, exists := rl.buckets[clientID]
	rl.mu.RUnlock()

	if exists {
		return bucket
	}

	rl.mu.Lock()
	defer rl.mu.Unlock()

	// Double-check after acquiring write lock
	if bucket, exists = rl.buckets[clientID]; exists {
		return bucket
	}

	bucket = NewTokenBucket(rl.config.RequestsPerSecond, rl.config.BurstSize)
	rl.buckets[clientID] = bucket
	return bucket
}

// Allow checks if a request from the given client is allowed
func (rl *RateLimiter) Allow(clientID string) bool {
	bucket := rl.GetBucket(clientID)
	return bucket.Allow()
}

// RateLimitMiddleware creates an Echo middleware for rate limiting
func RateLimitMiddleware(config RateLimitConfig) echo.MiddlewareFunc {
	limiter := NewRateLimiter(config)

	return func(next echo.HandlerFunc) echo.HandlerFunc {
		return func(c echo.Context) error {
			if !config.Enabled {
				return next(c)
			}

			// Extract client identifier
			// Priority: 1. X-Forwarded-For header (for proxied requests)
			//           2. X-User-ID header (for authenticated users)
			//           3. Remote address
			clientID := extractClientID(c)

			if !limiter.Allow(clientID) {
				bucket := limiter.GetBucket(clientID)
				remaining := int(bucket.GetTokens())
				retryAfter := int((1.0 / float64(limiter.config.RequestsPerSecond)) * float64(limiter.config.BurstSize))

				c.Response().Header().Set("X-RateLimit-Limit", strconv.Itoa(config.RequestsPerSecond))
				c.Response().Header().Set("X-RateLimit-Remaining", strconv.Itoa(remaining))
				c.Response().Header().Set("X-RateLimit-Reset", strconv.Itoa(retryAfter))
				c.Response().Header().Set("Retry-After", strconv.Itoa(retryAfter))

				return echo.NewHTTPError(http.StatusTooManyRequests, map[string]interface{}{
					"detail":     "Rate limit exceeded. Please slow down your requests.",
					"code":       "RATE_LIMIT_EXCEEDED",
					"retry_after": retryAfter,
				})
			}

			// Add rate limit headers to successful requests
			bucket := limiter.GetBucket(clientID)
			remaining := int(bucket.GetTokens())
			c.Response().Header().Set("X-RateLimit-Limit", strconv.Itoa(config.RequestsPerSecond))
			c.Response().Header().Set("X-RateLimit-Remaining", strconv.Itoa(remaining))

			return next(c)
		}
	}
}

// extractClientID extracts a unique client identifier from the request
func extractClientID(c echo.Context) string {
	// Check X-Forwarded-For first (for clients behind proxies)
	if xff := c.Request().Header.Get("X-Forwarded-For"); xff != "" {
		// Take the first IP in the chain (original client)
		if idx := indexByte(xff, ','); idx != -1 {
			return xff[:idx]
		}
		return xff
	}

	// Check X-Real-IP header
	if xri := c.Request().Header.Get("X-Real-IP"); xri != "" {
		return xri
	}

	// Fall back to authenticated user ID if available
	if userID := c.Request().Header.Get("X-User-ID"); userID != "" {
		return "user:" + userID
	}

	// Finally, use the remote address
	return c.RealIP()
}

// indexByte is a simple helper to find index of byte (avoids importing bytes package)
func indexByte(s string, c byte) int {
	for i := 0; i < len(s); i++ {
		if s[i] == c {
			return i
		}
	}
	return -1
}

// RouteRateLimitConfig holds per-route rate limit overrides
type RouteRateLimitConfig struct {
	Path         string
	RequestsPerSecond int
	BurstSize    int
	Enabled      bool
}

// CustomRateLimitMiddleware creates rate limiting middleware with per-route overrides
func CustomRateLimitMiddleware(defaultConfig RateLimitConfig, routeOverrides []RouteRateLimitConfig) echo.MiddlewareFunc {
	defaultLimiter := NewRateLimiter(defaultConfig)
	routeLimiters := make(map[string]*RateLimiter)

	for _, override := range routeOverrides {
		if override.Enabled {
			routeLimiters[override.Path] = NewRateLimitConfig(RateLimitConfig{
				RequestsPerSecond: override.RequestsPerSecond,
				BurstSize:        override.BurstSize,
				Enabled:          true,
			})
		}
	}

	return func(next echo.HandlerFunc) echo.HandlerFunc {
		return func(c echo.Context) error {
			path := c.Path()
			clientID := extractClientID(c)

			// Check for route-specific rate limiter
			limiter := defaultLimiter
			for routePath, routeLimiter := range routeLimiters {
				if hasPrefix(path, routePath) {
					limiter = routeLimiter
					break
				}
			}

			if !limiter.Allow(clientID) {
				bucket := limiter.GetBucket(clientID)
				remaining := int(bucket.GetTokens())

				c.Response().Header().Set("X-RateLimit-Limit", strconv.Itoa(limiter.config.RequestsPerSecond))
				c.Response().Header().Set("X-RateLimit-Remaining", strconv.Itoa(remaining))
				c.Response().Header().Set("Retry-After", "1")

				return echo.NewHTTPError(http.StatusTooManyRequests, map[string]interface{}{
					"detail": "Rate limit exceeded. Please slow down your requests.",
					"code":   "RATE_LIMIT_EXCEEDED",
				})
			}

			return next(c)
		}
	}
}

// NewRateLimitConfig is a helper to create RateLimitConfig
func NewRateLimitConfig(config RateLimitConfig) *RateLimiter {
	return NewRateLimiter(config)
}

// hasPrefix checks if s starts with prefix
func hasPrefix(s, prefix string) bool {
	return len(s) >= len(prefix) && s[:len(prefix)] == prefix
}
