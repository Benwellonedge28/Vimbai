package config

import (
	"encoding/json"
	"log"
	"os"
	"strconv"
	"strings"
)

// Route defines the configuration for a single microservice route
type Route struct {
	Path               string
	TargetURL          string
	AuthRequired       bool
	RateLimitPerSecond int // Per-route rate limit override (0 = use default)
	RateLimitBurst     int // Per-route burst override (0 = use default)
}

// RateLimitConfig holds global rate limiting configuration
type RateLimitConfig struct {
	Enabled           bool
	RequestsPerSecond int
	BurstSize         int
	// Per-route overrides
	RouteOverrides map[string]RouteRateLimit
}

// RouteRateLimit defines per-route rate limit settings
type RouteRateLimit struct {
	RequestsPerSecond int
	BurstSize         int
}

// Config holds the entire application configuration
type Config struct {
	Port                          int
	JwtSecret                     string
	IdentityServiceURL            string
	AccountingServiceURL          string
	FinanceServiceURL             string
	MultimodalServiceURL          string
	BankingIntegrationServiceURL  string
	SupplyChainServiceURL         string // RENAMED FROM InvoicingServiceURL
	FraudDetectionServiceURL     string
	Routes                        []Route
	RateLimit                     RateLimitConfig
}

// LoadConfig loads configuration from environment variables
func LoadConfig() *Config {
	portStr := os.Getenv("PORT")
	port, err := strconv.Atoi(portStr)
	if err != nil {
		port = 8081 // Default port
	}

	// Load rate limit configuration
	rateLimitEnabled := getEnv("RATE_LIMIT_ENABLED", "true") == "true"
	rateLimitRPS := getEnvInt("RATE_LIMIT_RPS", 100)
	rateLimitBurst := getEnvInt("RATE_LIMIT_BURST", 200)

	cfg := &Config{
		Port:                          port,
		JwtSecret:                     os.Getenv("JWT_SECRET"),
		IdentityServiceURL:            getEnv("IDENTITY_SERVICE_URL", "http://localhost:8080"),
		AccountingServiceURL:          getEnv("ACCOUNTING_SERVICE_URL", "http://localhost:8000"),
		FinanceServiceURL:            getEnv("FINANCE_SERVICE_URL", "http://localhost:8001"),
		MultimodalServiceURL:         getEnv("MULTIMODAL_SERVICE_URL", "http://localhost:8002"),
		BankingIntegrationServiceURL: getEnv("BANKING_INTEGRATION_SERVICE_URL", "http://localhost:8003"),
		SupplyChainServiceURL:        getEnv("SUPPLY_CHAIN_SERVICE_URL", "http://localhost:8004"), // RENAMED
		FraudDetectionServiceURL:     getEnv("FRAUD_DETECTION_SERVICE_URL", "http://localhost:8005"),
		RateLimit: RateLimitConfig{
			Enabled:           rateLimitEnabled,
			RequestsPerSecond: rateLimitRPS,
			BurstSize:         rateLimitBurst,
			RouteOverrides:    loadRouteRateLimits(),
		},
	}

	cfg.Routes = []Route{
		{Path: "/identity", TargetURL: cfg.IdentityServiceURL, AuthRequired: false},
		{Path: "/accounts", TargetURL: cfg.AccountingServiceURL, AuthRequired: true},
		{Path: "/journal-entries", TargetURL: cfg.AccountingServiceURL, AuthRequired: true},
		{Path: "/financial-statements", TargetURL: cfg.AccountingServiceURL, AuthRequired: true},
		{Path: "/ledgers", TargetURL: cfg.AccountingServiceURL, AuthRequired: true},
		{Path: "/trial-balance", TargetURL: cfg.AccountingServiceURL, AuthRequired: true},
		{Path: "/budgets", TargetURL: cfg.FinanceServiceURL, AuthRequired: true},
		{Path: "/financial-ratios", TargetURL: cfg.FinanceServiceURL, AuthRequired: true},
		{Path: "/multimodal", TargetURL: cfg.MultimodalServiceURL, AuthRequired: true},
		{Path: "/process-document-ocr", TargetURL: cfg.MultimodalServiceURL, AuthRequired: true},
		{Path: "/process-audio-to-text", TargetURL: cfg.MultimodalServiceURL, AuthRequired: true},
		{Path: "/process-multimodal-input", TargetURL: cfg.MultimodalServiceURL, AuthRequired: true},
		{Path: "/multimodal-to-journal-entry", TargetURL: cfg.MultimodalServiceURL, AuthRequired: true},
		{Path: "/tasks", TargetURL: cfg.MultimodalServiceURL, AuthRequired: true},
		{Path: "/banking-integration", TargetURL: cfg.BankingIntegrationServiceURL, AuthRequired: true},
		{Path: "/banks", TargetURL: cfg.BankingIntegrationServiceURL, AuthRequired: true},
		{Path: "/transactions", TargetURL: cfg.BankingIntegrationServiceURL, AuthRequired: true},
		{Path: "/customers", TargetURL: cfg.SupplyChainServiceURL, AuthRequired: true}, // ROUTED TO NEW SERVICE
		{Path: "/sales-invoices", TargetURL: cfg.SupplyChainServiceURL, AuthRequired: true}, // RENAMED ROUTE
		{Path: "/suppliers", TargetURL: cfg.SupplyChainServiceURL, AuthRequired: true}, // NEW ROUTE
		{Path: "/inventory-items", TargetURL: cfg.SupplyChainServiceURL, AuthRequired: true}, // NEW ROUTE
		{Path: "/purchase-orders", TargetURL: cfg.SupplyChainServiceURL, AuthRequired: true}, // NEW ROUTE
		{Path: "/fraud-detection", TargetURL: cfg.FraudDetectionServiceURL, AuthRequired: true},
	}

	// Merge dynamic routes from services.json if available
	dynamicRoutes := LoadRoutesFromFile("config/services.json")
	if len(dynamicRoutes) > 0 {
		cfg.Routes = append(cfg.Routes, dynamicRoutes...)
	}
	return cfg
}

func getEnv(key, defaultValue string) string {
	if value, exists := os.LookupEnv(key); exists {
		return value
	}
	log.Printf("Environment variable %s not set, using default value: %s", key, defaultValue)
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

// loadRouteRateLimits loads per-route rate limit overrides from environment
func loadRouteRateLimits() map[string]RouteRateLimit {
	overrides := make(map[string]RouteRateLimit)

	// Define stricter limits for sensitive endpoints
	sensitiveRoutes := []string{
		"/identity",
		"/auth",
		"/oauth",
	}

	for _, route := range sensitiveRoutes {
		rps := getEnvInt("RATE_LIMIT_"+sanitizeEnvKey(route)+"_RPS", 10)
		burst := getEnvInt("RATE_LIMIT_"+sanitizeEnvKey(route)+"_BURST", 20)
		if rps > 0 || burst > 0 {
			overrides[route] = RouteRateLimit{
				RequestsPerSecond: rps,
				BurstSize:         burst,
			}
		}
	}

	return overrides
}

// sanitizeEnvKey removes special characters for env var names
func sanitizeEnvKey(key string) string {
	result := strings.ReplaceAll(key, "-", "_")
	result = strings.ReplaceAll(result, "/", "_")
	result = strings.ToUpper(result)
	return result
}
// LoadRoutesFromFile loads additional routes from a services.json configuration file.
// This allows dynamic service registration without recompiling the gateway.
// The file should be located at config/services.json relative to the gateway binary.
func LoadRoutesFromFile(filePath string) []Route {
    data, err := os.ReadFile(filePath)
    if err != nil {
        log.Printf("Warning: could not read services.json at %s: %v", filePath, err)
        return nil
    }

    type serviceDef struct {
        Name         string `json:"name"`
        Path         string `json:"path"`
        URL          string `json:"url"`
        Port         int    `json:"port"`
        AuthRequired bool   `json:"auth_required"`
    }

    var config struct {
        Services []serviceDef `json:"services"`
    }

    if err := json.Unmarshal(data, &config); err != nil {
        log.Printf("Error parsing services.json: %v", err)
        return nil
    }

    routes := make([]Route, 0, len(config.Services))
    for _, svc := range config.Services {
        routes = append(routes, Route{
            Path:               svc.Path,
            TargetURL:          svc.URL,
            AuthRequired:       svc.AuthRequired,
            RateLimitPerSecond: 0,
            RateLimitBurst:      0,
        })
    }

    log.Printf("Loaded %d routes from services.json", len(routes))
    return routes
}
