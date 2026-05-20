package models

// Role defines user roles and their associated permissions.
type Role struct {
	ID          string   `json:"id"`
	Name        string   `json:"name"`        // e.g., "SUPER_ADMIN", "ACCOUNTANT", "FINANCE_LEAD"
	Permissions []string `json:"permissions"` // e.g., ["accounting.read", "finance.write.budget"]
}

// Predefined roles (for initial setup, later loaded from Graph DB)
var Roles = map[string]Role{
	"SUPER_ADMIN": {
		ID:          "1",
		Name:        "SUPER_ADMIN",
		Permissions: []string{"*.*"},
	},
	"FINANCE_LEAD": {
		ID:          "2",
		Name:        "FINANCE_LEAD",
		Permissions: []string{"finance.*", "accounting.read"},
	},
	"ACCOUNTANT": {
		ID:          "3",
		Name:        "ACCOUNTANT",
		Permissions: []string{"accounting.*"},
	},
	// ... more roles as defined in design doc
}
