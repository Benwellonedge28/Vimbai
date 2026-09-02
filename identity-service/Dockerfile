# Start with the official Go image
FROM golang:1.22-alpine

# Set the working directory inside the container
WORKDIR /app

# Create non-root user (Alpine uses -S for system, not -r)
RUN addgroup -S appuser && adduser -S -G appuser -H appuser

# Copy go.mod first for better layer caching
COPY go.mod ./

# Copy source code (needed for go mod tidy to see imports)
COPY . .

# Generate go.sum and download dependencies
RUN go mod tidy && go mod download

# Build the application
RUN go build -o /vimbai-identity-service ./main.go

# Switch to non-root user
USER appuser

# Expose the port the application will listen on
EXPOSE 8080

# Command to run the executable
CMD ["/vimbai-identity-service"]
