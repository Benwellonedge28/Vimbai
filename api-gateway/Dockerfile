FROM golang:1.20-alpine AS builder

WORKDIR /app

COPY go.mod go.sum ./
RUN go mod download

COPY . .

RUN go build -o main .

FROM alpine:latest

WORKDIR /app

COPY --from=builder /app/main .
COPY --from=builder /app/config ./config

EXPOSE 8081

CMD ["./main"]

RUN groupadd -r appuser && useradd -r -g appuser appuser
USER appuser
