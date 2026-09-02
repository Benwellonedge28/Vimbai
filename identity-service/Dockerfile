# Start with the official Go image
FROM golang:1.22-alpine

# Set the working directory inside the container
WORKDIR /app

# Create non-root user (Alpine uses addgroup/adduser, not groupadd/useradd)
RUN addgroup -r appuser && adduser -r -G appuser appuser

# Copy go.mod and go.sum files and download dependencies
COPY go.mod ./
COPY go.sum ./
RUN go mod download

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
