package main

import (
	"finacc/api-gateway/config"
	"finacc/api-gateway/middleware"
	"log"
	"net/http/httputil"
	"net/url"
	"strconv"
	"strings"

	"github.com/labstack/echo/v4"
	echoMiddleware "github.com/labstack/echo/v4/middleware"
)

func main() {
	cfg := config.LoadConfig()

	e := echo.New()

	// Add CORS middleware to allow requests from any origin (adjust for production)
	e.Use(echoMiddleware.CORSWithConfig(echoMiddleware.CORSConfig{
		AllowOrigins: []string{"*"},
		AllowMethods: []string{echo.GET, echo.HEAD, echo.PUT, echo.PATCH, echo.POST, echo.DELETE},
		AllowHeaders: []string{"Origin", "Content-Length", "Content-Type", "Authorization"},
	}))

	// JWT Authentication Middleware
	e.Use(middleware.AuthMiddleware(cfg))

	// Root endpoint for health check
	e.GET("/", func(c echo.Context) error {
		return c.JSON(http.StatusOK, map[string]string{"message": "FinAcc API Gateway is running!"})
	})

	// Dynamic Reverse Proxy Routes
	for _, route := range cfg.Routes {
		targetURL, err := url.Parse(route.TargetURL)
		if err != nil {
			log.Fatalf("Invalid target URL for path %s: %v", route.Path, err)
		}

		proxy := httputil.NewSingleHostReverseProxy(targetURL)
		e.Any(route.Path+"/":: ``, func(c echo.Context) error {
			c.Request().URL.Path = strings.TrimPrefix(c.Request().URL.Path, route.Path)
			proxy.ServeHTTP(c.Response(), c.Request())
			return nil
		})
		log.Printf("Proxying requests from %s to %s (Auth: %t)", route.Path, route.TargetURL, route.AuthRequired)
	}

	// Start server
	log.Printf("Starting FinAcc API Gateway on port %d", cfg.Port)
	e.Logger.Fatal(e.Start(":" + strconv.Itoa(cfg.Port)))
}
