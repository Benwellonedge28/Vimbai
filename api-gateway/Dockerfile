FROM golang:1.27-alpine AS builder

WORKDIR /app

COPY go.mod go.sum ./
RUN go mod download

COPY . .

RUN go build -o main .

FROM alpine:latest

WORKDIR /app

RUN addgroup -r appuser && adduser -r -G appuser appuser

COPY --from=builder /app/main .
COPY --from=builder /app/config ./config

USER appuser

EXPOSE 8081

CMD ["./main"]
