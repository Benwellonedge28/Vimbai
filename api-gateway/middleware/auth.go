package middleware

import (
	"finacc/api-gateway/config"
	"finacc/api-gateway/utils"
	"net/http"
	"strings"

	"github.com/labstack/echo/v4"
)

// AuthMiddleware checks for a valid JWT token
func AuthMiddleware(cfg *config.Config) echo.MiddlewareFunc {
	return func(next echo.HandlerFunc) echo.HandlerFunc {
		return func(c echo.Context) error {
			// Skip auth for routes that don't require it
			for _, route := range cfg.Routes {
				if !route.AuthRequired && strings.HasPrefix(c.Request().URL.Path, route.Path) {
					return next(c)
				}
			}

			authHeader := c.Request().Header.Get("Authorization")
			if authHeader == "" {
				return echo.NewHTTPError(http.StatusUnauthorized, "Authorization header is missing")
			}

			parts := strings.SplitN(authHeader, " ", 2)
			if len(parts) != 2 || strings.ToLower(parts[0]) != "bearer" {
				return echo.NewHTTPError(http.StatusUnauthorized, "Invalid Authorization header format")
			}

			tokenString := parts[1]
			claims, err := utils.ValidateToken(tokenString, cfg.JWTSecret)
			if err != nil {
				return echo.NewHTTPError(http.StatusUnauthorized, "Invalid or expired token: "+err.Error())
			}

			// Pass user claims to downstream services as headers
			c.Request().Header.Set("X-User-ID", claims.UserID)
			c.Request().Header.Set("X-Username", claims.Username)
			c.Request().Header.Set("X-User-Role", claims.Role)
			c.Request().Header.Set("X-User-Permissions", strings.Join(claims.Permissions, ","))

			return next(c)
		}
	}
}
