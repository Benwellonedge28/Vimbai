package main

import (
	"log"
	"net/http"

	"github.com/Benwellonedge28/FinAcc/identity-service/database" // NEW
	"github.com/Benwellonedge28/FinAcc/identity-service/handlers"
	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
)

func main() {
	// Initialize Neo4j connection
	database.InitNeo4j()
	defer database.CloseNeo4j() // Ensure driver is closed when app exits

	// Ensure schema constraints and seed initial roles
	database.EnsureSchema() // NEW
	database.SeedRoles()    // NEW

	router := chi.NewRouter()

	router.Use(middleware.Logger)
	router.Use(middleware.Recoverer)

	// Public routes for authentication
	router.Post("/register", handlers.RegisterUser)
	router.Post("/login", handlers.LoginUser)

	// Start the server
	log.Println("Identity Service starting on port 8080...")
	log.Fatal(http.ListenAndServe(":8080", router))
}
