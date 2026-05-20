package main

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"net/http/httputil"
	"net/url"
	"strconv"
	"strings"
	"time"

	"finacc/api-gateway/config"
	"finacc/api-gateway/middleware"

	"github.com/cenkalti/backoff/v4"
	"github.com/labstack/echo/v4"
	echoMiddleware "github.com/labstack/echo/v4/middleware"
	"github.com/sony/gobreaker"
)

// responseRecorder is a simple http.ResponseWriter that records the status code and body.
type responseRecorder struct {
	http.ResponseWriter
	Status    int
	Body      *bytes.Buffer
	HeaderMap http.Header
}

func (r *responseRecorder) Header() http.Header {
	if r.HeaderMap == nil {
		r.HeaderMap = make(http.Header)
	}
	return r.HeaderMap
}

func (r *responseRecorder) Write(b []byte) (int, error) {
	if r.Body == nil {
		r.Body = new(bytes.Buffer)
	}
	return r.Body.Write(b)
}

func (r *responseRecorder) WriteHeader(statusCode int) {
	r.Status = statusCode
}

// ProxyResilienceHandler encapsulates the proxy, circuit breaker, and retry logic.
type ProxyResilienceHandler struct {
	proxy        *httputil.ReverseProxy
	cb           *gobreaker.CircuitBreaker // nil if not used
	retryCfg     middleware.RetryConfig // zero value if not used
	authRequired bool
	routePath    string // Store original route path to trim correctly
}

// Handle implements the echo.HandlerFunc interface.
func (prh *ProxyResilienceHandler) Handle(c echo.Context) error {
	// Read the request body once and make it repeatable for retries
	reqBodyBytes, err := io.ReadAll(c.Request().Body)
	if err != nil {
		log.Printf("Error reading request body: %v", err)
		return echo.NewHTTPError(http.StatusInternalServerError, `{"detail": "Failed to read request body", "code": "REQUEST_BODY_ERROR"}`)
	}
	c.Request().Body = io.NopCloser(bytes.NewBuffer(reqBodyBytes)) // Restore for initial call if needed before retry/CB

	// If auth is not required, just proxy directly
	if !prh.authRequired {
		prh.proxy.ServeHTTP(c.Response(), c.Request())
		return nil
	}

	// --- Circuit Breaker & Retry Logic ---
	var finalRecordedResp *responseRecorder // Stores the response from the successful/final retry
	var lastErr error                     // Stores the last error from a retry attempt

	// Function that executes the actual proxy call, potentially retrying
	resilientProxyCall := func() (interface{}, error) {
		// Function for a single proxy attempt, wrapped by retry
		singleAttempt := func() error {
			// Clone the request for each attempt and reset its body
			attemptReq := c.Request().Clone(c.Request().Context())
			attemptReq.Body = io.NopCloser(bytes.NewBuffer(reqBodyBytes)) // Restore body for this attempt

			// Create a fresh recorder for each attempt
			recorder := &responseRecorder{ResponseWriter: c.Response().Writer}
			recorder.Body = new(bytes.Buffer) // Initialize body buffer

			prh.proxy.ServeHTTP(recorder, attemptReq)

			if recorder.Status == 0 { // Default to 200 OK if not explicitly set
				recorder.Status = http.StatusOK
			}

			// Check for retryable status codes
			for _, statusCode := range prh.retryCfg.RetryableStatusCodes {
				if recorder.Status == statusCode {
					log.Printf("Request to %s%s resulted in retryable status %d, retrying...\n", prh.routePath, attemptReq.URL.Path, recorder.Status)
					return fmt.Errorf("retryable HTTP status: %d", recorder.Status) // Signal to backoff to retry
				}
			}
			
			// For non-retryable errors (e.g., 4xx, or non-retryable 5xx)
			if recorder.Status >= 400 {
				finalRecordedResp = recorder // Capture response for immediate return
				return backoff.Permanent(echo.NewHTTPError(recorder.Status, recordedResponse.Body.String())) // Permanent error, stop retrying
			}

			// Success case
			finalRecordedResp = recorder // Capture response
			return nil
		}

		b := backoff.NewExponentialBackOff()
		b.InitialInterval = prh.retryCfg.InitialInterval
		b.MaxInterval = prh.retryCfg.MaxInterval
		b.Multiplier = prh.retryCfg.Multiplier
		b.MaxElapsedTime = time.Duration(prh.retryCfg.MaxAttempts) * prh.retryCfg.InitialInterval

		err := backoff.Retry(singleAttempt, backoff.WithMaxRetries(b, prh.retryCfg.MaxAttempts))
		if err != nil {
			lastErr = err // Capture the error that caused retry to stop
			return nil, err // Return to circuit breaker to mark failure
		}
		return nil, nil // Successful response after possible retries
	}

	// Execute the resilient proxy call within the circuit breaker
	_, cbErr := prh.cb.Execute(resilientProxyCall)
	if cbErr != nil {
		if cbErr == gobreaker.ErrOpenState {
			log.Printf("Circuit breaker open for %s, request to %s rejected.\n", prh.cb.Name(), c.Request().URL.Path)
			return echo.NewHTTPError(http.StatusServiceUnavailable, fmt.Sprintf(`{"detail": "Service unavailable (circuit breaker is open for %s)", "code": "SERVICE_UNAVAILABLE_CB"}`, prh.cb.Name()))
		}
		// If the error came from the retry mechanism (lastErr)
		if lastErr != nil {
			// If it's an Echo HTTPError from backoff.Permanent
			if httpErr, ok := lastErr.(*echo.HTTPError); ok {
				return httpErr
			}
			// Generic internal server error for other unexpected retry errors
			return echo.NewHTTPError(http.StatusInternalServerError, fmt.Sprintf(`{"detail": "Upstream service failed after retries: %v", "code": "UPSTREAM_FAILURE_RETRY"}`, lastErr))
		}
		// Catch-all for other unexpected circuit breaker errors
		return echo.NewHTTPError(http.StatusInternalServerError, fmt.Sprintf(`{"detail": "Gateway resilience error: %v", "code": "GATEWAY_RESILIENCE_ERROR"}`, cbErr))
	}

	// If successful, write the recorded response to the actual client response writer
	if finalRecordedResp != nil {
		for k, v := range finalRecordedResp.Header() {
			c.Response().Header()[k] = v
		}
		c.Response().WriteHeader(finalRecordedResp.Status)
		if finalRecordedResp.Body != nil {
			c.Response().Write(finalRecordedResp.Body.Bytes())
		}
	} else {
		// This case should ideally not be hit if everything works, implies direct proxy call without resilience.
		// But if it was a non-auth route, it would be handled earlier.
		// This is a safety fallback.
		prh.proxy.ServeHTTP(c.Response(), c.Request())
	}

	return nil
}

// Main function
func main() {
	cfg := config.LoadConfig()
	e := echo.New()

	e.Use(echoMiddleware.Logger())
	e.Use(echoMiddleware.Recover()) // Catches panics from proxy.ErrorHandler and converts to 500
	e.Use(echoMiddleware.CORSWithConfig(echoMiddleware.CORSConfig{
		AllowOrigins: []string{"*"},
		AllowMethods: []string{echo.GET, echo.HEAD, echo.PUT, echo.PATCH, echo.POST, echo.DELETE},
		AllowHeaders: []string{"Origin", "Content-Length", "Content-Type", "Authorization", "X-User-ID", "X-Username", "X-User-Role", "X-User-Permissions"},
	}))

	// Global JWT Authentication Middleware
	e.Use(middleware.AuthMiddleware(cfg))

	e.GET("/", func(c echo.Context) error {
		return c.JSON(http.StatusOK, map[string]string{"message": "FinAcc API Gateway is running!"})
	})

	// Initialize Circuit Breakers for each upstream service
	circuitBreakers := make(map[string]*gobreaker.CircuitBreaker)
	for _, route := range cfg.Routes {
		if route.AuthRequired { 
			if _, exists := circuitBreakers[route.TargetURL]; !exists {
				settings := gobreaker.Settings{
					Name:        strings.ReplaceAll(strings.TrimPrefix(route.TargetURL, "http://"), ":", "_"),
					MaxRequests: middleware.DefaultCircuitBreakerConfig.MaxRequests,
					Interval:    middleware.DefaultCircuitBreakerConfig.Interval,
					Timeout:     middleware.DefaultCircuitBreakerConfig.Timeout,
					ReadyToOpen: middleware.DefaultCircuitBreakerConfig.ReadyToOpen,
					OnStateChange: middleware.DefaultCircuitBreakerConfig.OnStateChange,
				}
				circuitBreakers[route.TargetURL] = gobreaker.NewCircuitBreaker(settings)
				log.Printf("Initialized circuit breaker '%s' for %s\n", settings.Name, route.TargetURL)
			}
		}
	}

	// Setup routes with resilience
	for _, route := range cfg.Routes {
		targetURL, err := url.Parse(route.TargetURL)
		if err != nil {
			log.Fatalf("Invalid target URL for path %s: %v", route.Path, err)
		}

		// Create a custom reverse proxy for each route
		rp := httputil.NewSingleHostReverseProxy(targetURL)
		rp.Director = func(req *http.Request) {
			req.URL.Scheme = targetURL.Scheme
			req.URL.Host = targetURL.Host
			req.Host = targetURL.Host 
			req.URL.Path = strings.TrimPrefix(req.URL.Path, route.Path)
			if _, ok := req.Header["User-Agent"]; !ok {
				req.Header.Set("User-Agent", "FinAcc-API-Gateway")
			}
			if clientIP, _, err := net.SplitHostPort(req.RemoteAddr); err == nil {
				if priorXFF := req.Header.Get("X-Forwarded-For"); priorXFF != "" {
					req.Header.Set("X-Forwarded-For", priorXFF+", "+clientIP)
				} else {
					req.Header.Set("X-Forwarded-For", clientIP)
				}
			}
		}
		// Custom Transport for proxy to log and potentially classify errors for CB
		rp.Transport = &http.Transport{
			Proxy: http.ProxyFromEnvironment,
			DialContext: (&net.Dialer{
				Timeout:   30 * time.Second,
				KeepAlive: 30 * time.Second,
			}).DialContext,
			ForceAttemptHTTP2:     true,
			MaxIdleConns:          100,
			IdleConnTimeout:       90 * time.Second,
			TLSHandshakeTimeout:   10 * time.Second,
			ExpectContinueTimeout: 1 * time.Second,
		}

		// Custom ErrorHandler for proxy transport errors
		rp.ErrorHandler = func(w http.ResponseWriter, r *http.Request, err error) {
			log.Printf("Proxy.ErrorHandler: Transport error to %s for %s%s: %v\n", targetURL.String(), route.Path, r.URL.Path, err)
			// httputil.ReverseProxy's ErrorHandler is called for transport-level errors (e.g., connection refused).
			// We need to signal this error back to the circuit breaker. This is done by propagating the error.
			// For simplicity, we just return an HTTPError here which Recover middleware will catch.
			ec := c.Echo()
			ec.DefaultHTTPErrorHandler(err, c) // Use Echo's default error handler for proxy transport errors
			if route.AuthRequired && circuitBreakers[route.TargetURL] != nil {
				circuitBreakers[route.TargetURL].Fail() // Manually mark as failure for the circuit breaker
			}
		}

		// Define the resilience handler for the route
		handler := &ProxyResilienceHandler{
			proxy:        rp,
			authRequired: route.AuthRequired,
			routePath:    route.Path,
		}

		if route.AuthRequired {
			handler.cb = circuitBreakers[route.TargetURL]
			handler.retryCfg = middleware.DefaultRetryConfig
			if handler.cb == nil {
				log.Fatalf("Circuit breaker not found for target %s, but AuthRequired is true", route.TargetURL)
			}
		}

		e.Any(route.Path+"*", handler.Handle)
		log.Printf("Proxying requests from %s to %s (Auth: %t, CB/Retry: %t)\n", route.Path, route.TargetURL, route.AuthRequired, route.AuthRequired)
	}

	log.Printf("Starting FinAcc API Gateway on port %d\n", cfg.Port)
	e.Logger.Fatal(e.Start(":" + strconv.Itoa(cfg.Port)))
}
