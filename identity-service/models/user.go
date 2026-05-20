package models

// User represents a user in the system.
type User struct {
	ID       string `json:"id"`
	Username string `json:"username"`
	Password string `json:"password"` // Hashed password
	Email    string `json:"email"`
	RoleID   string `json:"role_id"` // Link to Role
}
