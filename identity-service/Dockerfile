# Start with the official Go image
FROM golang:1.22-alpine

# Set the working directory inside the container
WORKDIR /app

# Create non-root user (Alpine uses -S for system, not -r)
RUN addgroup -S appuser && adduser -S -G appuser -H appuser

# Copy go.mod and download dependencies
COPY go.mod ./
RUN go mod download || true

# Copy the rest of the application source code
COPY . .

# Build the application
RUN go build -o /vimbai-identity-service ./main.go

# Switch to non-root user
USER appuser

# Expose the port the application will listen on
EXPOSE 8080

# Command to run the executable
CMD ["/vimbai-identity-service"]
