package models

// User represents a user in the system.
type User struct {
	ID          string   `json:"id"`
	Username    string   `json:"username"`
	Email       string   `json:"email"`
	RoleID      string   `json:"role_id"`               // Storing assigned role's ID or name
	Permissions []string `json:"permissions,omitempty"` // Derived from Role
}
