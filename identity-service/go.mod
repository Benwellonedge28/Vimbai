module github.com/Benwellonedge28/Vimbai/identity-service

go 1.22.0

require (
	github.com/dgrijalva/jwt-go v3.2.0+incompatible
	github.com/go-chi/chi/v5 v5.0.12
	github.com/neo4j/neo4j-go-driver/v4 v4.4.7 // NEW: Neo4j Go Driver
	golang.org/x/crypto v0.23.0 // For bcrypt password hashing
)
