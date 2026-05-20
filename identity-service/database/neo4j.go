package database

import (
	"fmt"
	"log"
	"os"

	"github.com/Benwellonedge28/FinAcc/identity-service/models"
	"github.com/neo4j/neo4j-go-driver/v4/neo4j"
)

var (
	Driver neo4j.Driver
)

// InitNeo4j initializes the Neo4j driver.
func InitNeo4j() {
	neo4jUri := os.Getenv("NEO4J_URI")
	neo4jUser := os.Getenv("NEO4J_USER")
	neo4jPassword := os.Getenv("NEO4J_PASSWORD")

	if neo4jUri == "" {
		log.Fatal("NEO4J_URI environment variable not set")
	}
	if neo4jUser == "" {
		log.Fatal("NEO4J_USER environment variable not set")
	}
	if neo4jPassword == "" {
		log.Fatal("NEO4J_PASSWORD environment variable not set")
	}

	auth := neo4j.BasicAuth(neo4jUser, neo4jPassword, "")
	var err error
	Driver, err = neo4j.NewDriver(neo4jUri, auth)
	if err != nil {
		log.Fatalf("Error creating Neo4j driver: %v", err)
	}

	// Verify the connection
	err = Driver.VerifyConnectivity()
	if err != nil {
		log.Fatalf("Error verifying Neo4j connectivity: %v", err)
	}

	log.Println("Successfully connected to Neo4j database.")
}

// CloseNeo4j closes the Neo4j driver.
func CloseNeo4j() {
	if Driver != nil {
		Driver.Close()
		log.Println("Neo4j driver closed.")
	}
}

// EnsureSchema creates necessary constraints for the Identity Service
func EnsureSchema() {
	session := Driver.NewSession(neo4j.SessionConfig{AccessMode: neo4j.AccessModeWrite})
	defer session.Close()

	_, err := session.WriteTransaction(func(tx neo4j.Transaction) (interface{}, error) {
		// Create unique constraint on User.username
		if _, err := tx.Run("CREATE CONSTRAINT ON (u:User) ASSERT u.username IS UNIQUE", nil); err != nil {
			return nil, fmt.Errorf("failed to create User username constraint: %w", err)
		}
		// Create unique constraint on Role.name
		if _, err := tx.Run("CREATE CONSTRAINT ON (r:Role) ASSERT r.name IS UNIQUE", nil); err != nil {
			return nil, fmt.Errorf("failed to create Role name constraint: %w", err)
		}
		return nil, nil
	})
	if err != nil {
		log.Fatalf("Failed to ensure Neo4j schema constraints: %v", err)
	}
	log.Println("Neo4j schema constraints ensured.")
}

// SeedRoles creates default roles if they don't exist.
func SeedRoles() {
	session := Driver.NewSession(neo4j.SessionConfig{AccessMode: neo4j.AccessModeWrite})
	defer session.Close()

	for _, role := range models.Roles { // models.Roles defined in models/role.go
		_, err := session.WriteTransaction(func(tx neo4j.Transaction) (interface{}, error) {
			query := `
				MERGE (r:Role {name: $name})
				ON CREATE SET r.id = $id, r.permissions = $permissions
				RETURN r
			`
			_, err := tx.Run(query, map[string]interface{}{
				"id":          role.ID,
				"name":        role.Name,
				"permissions": role.Permissions,
			})
			return nil, err
		})
		if err != nil {
			log.Printf("Warning: Failed to seed role %s: %v", role.Name, err)
		} else {
			log.Printf("Role %s seeded successfully (or already exists).", role.Name)
		}
	}
}
