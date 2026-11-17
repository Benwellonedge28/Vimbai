package middleware

import (
	"net/http"
	"strings"
	"vimbai/api-gateway/config"
	"vimbai/api-gateway/utils"

	"github.com/labstack/echo/v4"
)

// AuthMiddleware checks for a valid JWT token
func AuthMiddleware(cfg *config.Config) echo.MiddlewareFunc {
	return func(next echo.HandlerFunc) echo.HandlerFunc {
		return func(c echo.Context) error {
			// Who the caller is comes from the token and from nowhere else. Whatever a client put in these headers is
			// dropped before any route runs, including the routes that need no token: the engine believes these headers,
			// so passing one through unaudited would let a caller name itself.
			stripCallerHeaders(c)

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
			claims, err := utils.ValidateToken(tokenString, cfg.JwtSecret)
			if err != nil {
				return echo.NewHTTPError(http.StatusUnauthorized, "Invalid or expired token: "+err.Error())
			}

			// Name the caller for the engine, from the token just verified.
			c.Request().Header.Set("X-User-ID", claims.UserID)
			c.Request().Header.Set("X-Username", claims.Username)
			c.Request().Header.Set("X-User-Role", claims.Role)
			c.Request().Header.Set("X-User-Permissions", strings.Join(claims.Permissions, ","))

			return next(c)
		}
	}
}

// callerHeaders are the headers the engine reads to learn who is acting. They belong to the gateway alone.
var callerHeaders = []string{"X-User-ID", "X-User-Id", "X-Username", "X-User-Role", "X-User-Permissions"}

// stripCallerHeaders removes any caller identity a client sent, so the only identity downstream is the one the gateway
// sets after verifying a token.
func stripCallerHeaders(c echo.Context) {
	for _, header := range callerHeaders {
		c.Request().Header.Del(header)
	}
}
