package middleware

import (
	"fmt"
	"net/http"
	"time"

	"github.com/cenkalti/backoff/v4"
	"github.com/labstack/echo/v4"
)

// RetryConfig defines configuration for the retry middleware.
type RetryConfig struct {
	// MaxAttempts is the maximum number of times to retry a failed request.
	MaxAttempts uint64
	// InitialInterval is the initial backoff interval.
	InitialInterval time.Duration
	// MaxInterval is the maximum backoff interval.
	MaxInterval time.Duration
	// Multiplier is the factor by which the interval increases.
	Multiplier float64
	// RetryableStatusCodes are HTTP status codes that should trigger a retry.
	RetryableStatusCodes []int
}

// DefaultRetryConfig provides default values for retry configuration.
var DefaultRetryConfig = RetryConfig{
	MaxAttempts:          3,
	InitialInterval:      100 * time.Millisecond,
	MaxInterval:          2 * time.Second,
	Multiplier:           1.5,
	RetryableStatusCodes: []int{http.StatusRequestTimeout, http.StatusServiceUnavailable, http.StatusGatewayTimeout, http.StatusBadGateway, http.StatusInternalServerError},
}

// RetryMiddleware creates a middleware that retries requests upon certain HTTP status codes or transient errors.
// This middleware is not directly used in main.go anymore; its logic is integrated into ProxyResilienceHandler.
func RetryMiddleware(config RetryConfig) echo.MiddlewareFunc {
	return func(next echo.HandlerFunc) echo.HandlerFunc {
		return func(c echo.Context) error {
			var lastErr error
			operation := func() error {
				// We cannot easily reset Echo's response writer and buffer for each retry attempt
				// without significant custom implementations. This means if a non-idempotent request
				// fails after partial writing, retrying might send a duplicate.
				// For a true transparent retry with Echo, a custom HTTP client and RoundTripper would be better.

				err := next(c)
				if err != nil {
					lastErr = err
					// Check if it's an HTTP error with a retryable status code
					if httpError, ok := err.(*echo.HTTPError); ok {
						for _, statusCode := range config.RetryableStatusCodes {
							if httpError.Code == statusCode {
								fmt.Printf("Retrying request to %s due to status %d\n", c.Request().URL.Path, httpError.Code)
								return fmt.Errorf("retryable HTTP error: %d", httpError.Code) // Trigger retry
							}
						}
					}
					// If it's a non-retryable error, stop retrying
					return backoff.Permanent(err)
				}

				// If no error was returned by next(c), it means the request was successfully processed by Echo.
				// We cannot inspect the http.ResponseWriter's status code here reliably as it might be committed.
				// The logic is integrated directly into the ProxyResilienceHandler for better control.
				return nil
			}

			b := backoff.NewExponentialBackOff()
			b.InitialInterval = config.InitialInterval
			b.MaxInterval = config.MaxInterval
			b.Multiplier = config.Multiplier
			b.MaxElapsedTime = config.MaxInterval * time.Duration(config.MaxAttempts) // Estimate max elapsed time

			err := backoff.Retry(operation, backoff.WithMaxRetries(b, config.MaxAttempts))
			if err != nil {
				// If all retries failed, return the last error encountered
				if lastErr != nil {
					return lastErr
				}
				return echo.NewHTTPError(http.StatusInternalServerError, "Failed after multiple retries: "+err.Error())
			}
			return nil
		}
	}
}
