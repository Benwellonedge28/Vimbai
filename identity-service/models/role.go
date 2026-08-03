package models

// Role defines user roles and their associated permissions.
type Role struct {
	ID          string   `json:"id"`
	Name        string   `json:"name"`        // e.g., "SUPER_ADMIN", "ACCOUNTANT", "FINANCE_LEAD"
	Permissions []string `json:"permissions"` // e.g., ["accounting.read", "finance.write.budget"]
}

// Predefined roles used for seeding the Graph DB.
// These are the "master" definitions for roles and their capabilities.
var Roles = map[string]Role{
	"SUPER_ADMIN": {
		ID:          "1", // Using fixed IDs for seeded roles
		Name:        "SUPER_ADMIN",
		Permissions: []string{"*.*"}, // Wildcard for all permissions
	},
	"FINANCE_LEAD": {
		ID:          "2",
		Name:        "FINANCE_LEAD",
		Permissions: []string{"finance.read", "finance.write.budget", "accounting.read"}, // Specific permissions
	},
	"ACCOUNTANT": {
		ID:          "3",
		Name:        "ACCOUNTANT",
		Permissions: []string{"accounting.read", "accounting.write.journal", "accounting.write.reconciliation"},
	},
	"AUDITOR": {
		ID:          "4",
		Name:        "AUDITOR",
		Permissions: []string{"*.read"}, // Read-only across all services
	},
	"POS_SERVICE": { // This would typically be a service account, not a human user
		ID:          "5",
		Name:        "POS_SERVICE",
		Permissions: []string{"integration.write.pos"}, // Permission to write POS transactions
	},
	"CLERK": {
		ID:          "6",
		Name:        "CLERK",
		Permissions: []string{"multimodal.input", "accounting.write.pending"},
	},
	// Add more roles as defined in Vimbai Design Document V1.5, Section 10.2
}
