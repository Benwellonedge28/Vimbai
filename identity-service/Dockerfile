# Start with the official Go image
FROM golang:1.26-alpine

# Set the working directory inside the container
WORKDIR /app

# Copy go.mod and go.sum files and download dependencies
COPY go.mod ./
COPY go.sum ./
RUN go mod download

# Copy the rest of the application source code
COPY . .

# Build the application
RUN go build -o /vimbai-identity-service ./main.go

# Expose the port the application will listen on
EXPOSE 8080

# Command to run the executable
CMD ["/vimbai-identity-service"]
