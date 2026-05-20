package config

import (
	"os"
	"strconv"
)

// Route defines a proxy route
type Route struct {
	Path        string
	TargetURL   string
	AuthRequired bool
}

// Config holds the gateway configuration
type Config struct {
	Port         int
	JWTSecret    string
	IdentityServiceURL string
	Routes       []Route
}

// LoadConfig loads configuration from environment variables
func LoadConfig() *Config {
	portStr := os.Getenv("PORT")
	port, err := strconv.Atoi(portStr)
	if err != nil {
		port = 8081 // Default port for gateway
	}

	return &Config{
		Port:         port,
		JWTSecret:    os.Getenv("JWT_SECRET"),
		IdentityServiceURL: os.Getenv("IDENTITY_SERVICE_URL"),
		Routes: []Route{
			{Path: "/identity", TargetURL: os.Getenv("IDENTITY_SERVICE_URL"), AuthRequired: false},
			{Path: "/accounts", TargetURL: os.Getenv("ACCOUNTING_SERVICE_URL"), AuthRequired: true},
			{Path: "/journal-entries", TargetURL: os.Getenv("ACCOUNTING_SERVICE_URL"), AuthRequired: true},
			{Path: "/ledger", TargetURL: os.Getenv("ACCOUNTING_SERVICE_URL"), AuthRequired: true},
			{Path: "/trial-balance", TargetURL: os.Getenv("ACCOUNTING_SERVICE_URL"), AuthRequired: true},
			{Path: "/financial-statements", TargetURL: os.Getenv("ACCOUNTING_SERVICE_URL"), AuthRequired: true},
			{Path: "/budgets", TargetURL: os.Getenv("FINANCE_SERVICE_URL"), AuthRequired: true},
			{Path: "/financial-ratios", TargetURL: os.Getenv("FINANCE_SERVICE_URL"), AuthRequired: true},
			{Path: "/process-document-ocr", TargetURL: os.Getenv("MULTIMODAL_SERVICE_URL"), AuthRequired: true},
			{Path: "/process-audio-to-text", TargetURL: os.Getenv("MULTIMODAL_SERVICE_URL"), AuthRequired: true},
			{Path: "/process-multimodal-input", TargetURL: os.Getenv("MULTIMODAL_SERVICE_URL"), AuthRequired: true},
			{Path: "/multimodal-to-journal-entry", TargetURL: os.Getenv("MULTIMODAL_SERVICE_URL"), AuthRequired: true},
            {Path: "/banking", TargetURL: os.Getenv("BANKING_INTEGRATION_SERVICE_URL"), AuthRequired: true}, // NEW
			// Add other service routes here
		},
	}
}
