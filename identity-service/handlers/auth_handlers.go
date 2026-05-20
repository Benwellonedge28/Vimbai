package handlers

import (
	"encoding/json"
	"fmt"
	"net/http"
	"time"

	"github.com/Benwellonedge28/FinAcc/identity-service/models"
	"github.com/Benwellonedge28/FinAcc/identity-service/utils"
	"golang.org/x/crypto/bcrypt"
)

// Mock "database" for demonstration. In a real scenario, this would interact with the Graph DB.
var users = make(map[string]models.User)

// RegisterUser handles new user registration.
func RegisterUser(w http.ResponseWriter, r *http.Request) {
	var user models.User
	if err := json.NewDecoder(r.Body).Decode(&user); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	if _, exists := users[user.Username]; exists {
		http.Error(w, "Username already taken", http.StatusConflict)
		return
	}

	// Hash password
	hashedPassword, err := bcrypt.GenerateFromPassword([]byte(user.Password), bcrypt.DefaultCost)
	if err != nil {
		http.Error(w, "Failed to hash password", http.StatusInternalServerError)
		return
	}
	user.Password = string(hashedPassword)
	user.ID = fmt.Sprintf("user-%d", time.Now().UnixNano()) // Simple ID generation
	
	// Assign a default role for now (e.g., Accountant)
	if user.RoleID == "" {
		user.RoleID = models.Roles["ACCOUNTANT"].ID 
	}

	users[user.Username] = user

	w.WriteHeader(http.StatusCreated)
	json.NewEncoder(w).Encode(map[string]string{"message": "User registered successfully", "user_id": user.ID})
}

// LoginUser handles user login and JWT generation.
func LoginUser(w http.ResponseWriter, r *http.Request) {
	var credentials struct {
		Username string `json:"username"`
		Password string `json:"password"`
	}
	if err := json.NewDecoder(r.Body).Decode(&credentials); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	user, exists := users[credentials.Username]
	if !exists {
		http.Error(w, "Invalid credentials", http.StatusUnauthorized)
		return
	}

	if err := bcrypt.CompareHashAndPassword([]byte(user.Password), []byte(credentials.Password)); err != nil {
		http.Error(w, "Invalid credentials", http.StatusUnauthorized)
		return
	}

	// Retrieve user role and permissions
	var userRole models.Role
	for _, role := range models.Roles {
		if role.ID == user.RoleID {
			userRole = role
			break
		}
	}

	// Generate JWT token
	token, err := utils.GenerateJWT(user.ID, user.Username, userRole.Name, userRole.Permissions)
	if err != nil {
		http.Error(w, "Failed to generate token", http.StatusInternalServerError)
		return
	}

	json.NewEncoder(w).Encode(map[string]string{"token": token})
}
