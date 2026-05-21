package config

import (
	"log"
	"os"
	"strconv"
)

// Route defines the configuration for a single microservice route
type Route struct {
	Path         string
	TargetURL    string
	AuthRequired bool
}

// Config holds the entire application configuration
type Config struct {
	Port                int
	JwtSecret           string
	IdentityServiceURL  string
	AccountingServiceURL string
	FinanceServiceURL   string
	MultimodalServiceURL string
	BankingIntegrationServiceURL string
	SupplyChainServiceURL string // RENAMED FROM InvoicingServiceURL
	FraudDetectionServiceURL string
	Routes              []Route
}

// LoadConfig loads configuration from environment variables
func LoadConfig() *Config {
	portStr := os.Getenv("PORT")
	port, err := strconv.Atoi(portStr)
	if err != nil {
		port = 8081 // Default port
	}

	cfg := &Config{
		Port:                port,
		JwtSecret:           getEnv("JWT_SECRET", "your_super_secret_jwt_key"),
		IdentityServiceURL:  getEnv("IDENTITY_SERVICE_URL", "http://localhost:8080"),
		AccountingServiceURL: getEnv("ACCOUNTING_SERVICE_URL", "http://localhost:8000"),
		FinanceServiceURL:   getEnv("FINANCE_SERVICE_URL", "http://localhost:8001"),
		MultimodalServiceURL: getEnv("MULTIMODAL_SERVICE_URL", "http://localhost:8002"),
		BankingIntegrationServiceURL: getEnv("BANKING_INTEGRATION_SERVICE_URL", "http://localhost:8003"),
		SupplyChainServiceURL: getEnv("SUPPLY_CHAIN_SERVICE_URL", "http://localhost:8004"), // RENAMED
		FraudDetectionServiceURL: getEnv("FRAUD_DETECTION_SERVICE_URL", "http://localhost:8005"),
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

	return cfg
}

func getEnv(key, defaultValue string) string {
	if value, exists := os.LookupEnv(key); exists {
		return value
	}
	log.Printf("Environment variable %s not set, using default value: %s", key, defaultValue)
	return defaultValue
}
