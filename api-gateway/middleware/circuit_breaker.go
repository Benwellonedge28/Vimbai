package middleware

import (
	"fmt"
	"net/http"
	"time"

	"github.com/labstack/echo/v4"
	"github.com/sony/gobreaker"
)

// CircuitBreakerConfig defines configuration for the circuit breaker middleware.
type CircuitBreakerConfig struct {
	// Name of the circuit breaker for metrics/logging.
	Name string
	// MaxRequests is the maximum number of requests allowed to pass through
	// when the circuit is half-open.
	MaxRequests uint32
	// Interval is the cyclic period of the closed state for the circuit breaker
	// to clear the internal counts of successive failures and successes.
	Interval time.Duration
	// Timeout is the period of time to wait before allowing a single trial request to pass through
	// when the circuit is open.
	Timeout time.Duration
	// ReadyToOpen is called whenever a Stats' consecutive failure count reaches MaxFailures.
	// If it returns true, the state of the breaker becomes open.
	ReadyToOpen func(counts gobreaker.Counts) bool
	// OnStateChange is called whenever the state changes.
	OnStateChange func(name string, from, to gobreaker.State)
}

// DefaultCircuitBreakerConfig provides default values for circuit breaker configuration.
var DefaultCircuitBreakerConfig = CircuitBreakerConfig{
	Name:        "default",
	MaxRequests: 1,
	Interval:    5 * time.Second,
	Timeout:     10 * time.Second,
	ReadyToOpen: func(counts gobreaker.Counts) bool {
		// Open the circuit if 60% or more of requests fail in a given interval (min 3 requests)
		return counts.Request > 3 && float64(counts.Failure)/float64(counts.Request) >= 0.6
	},
	OnStateChange: func(name string, from, to gobreaker.State) {
		fmt.Printf("Circuit Breaker '%s' changed from %s to %s\n", name, from.String(), to.String())
	},
}

// CircuitBreakerMiddleware creates a middleware that wraps the handler with a circuit breaker.
// This middleware is not directly used in main.go anymore; its logic is integrated into ProxyResilienceHandler.
func CircuitBreakerMiddleware(cb *gobreaker.CircuitBreaker) echo.MiddlewareFunc {
	return func(next echo.HandlerFunc) echo.HandlerFunc {
		return func(c echo.Context) error {
			_, err := cb.Execute(func() (interface{}, error) {
				// The actual request execution. If the next handler returns an error,
				// that error must be returned here to be counted by the circuit breaker.
				return nil, next(c)
			})

			if err != nil {
				// If the circuit is open, gobreaker.ErrOpenState is returned.
				if err == gobreaker.ErrOpenState {
					return echo.NewHTTPError(http.StatusServiceUnavailable, fmt.Sprintf("Service unavailable (circuit breaker is open for %s)", cb.Name()))
				}
				// Other gobreaker errors or errors from next(c)
				return err
			}
			return nil
		}
	}
}
