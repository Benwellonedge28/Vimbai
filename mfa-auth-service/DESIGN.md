# Multi-Factor Authentication Service — Design Document

## Overview

The Multi-Factor Authentication (MFA) Service enforces strong security boundaries for sensitive actions within Vimbai. Because Vimbai's privacy model relies entirely on user-controlled encryption keys, robust authentication is the only way to protect those keys from unauthorized access.

## Authentication Factors

Vimbai supports a layered approach to authentication:
1. **Knowledge:** Passwords, PINs, or Recovery Phrases.
2. **Biometrics:** Fingerprint or Face ID (using device-native APIs; raw biometric data is never transmitted to or stored by Vimbai).
3. **Possession:** Trusted devices, Passkeys, or hardware security keys.

## Context-Aware Security

The service requires different levels of authentication depending on the action:
- **Normal Login:** Passkey OR (Password + Biometric)
- **Approve Expense:** Biometric confirmation
- **Restore Backup:** High security (Password + Biometric) OR Recovery Phrase
- **Add New Device:** QR code approval from an existing trusted device + Biometric

## Endpoints

| Method | Path | Description |
| :--- | :--- | :--- |
| `GET` | `/health` | Health check |
| `POST` | `/auth/verify` | Verify authentication factors against a requested action |

## Security Principle

> "Your identity unlocks access, but your encryption keys protect your data."

This service verifies identity. Once identity is proven, the client application is authorized to use the local encryption keys to decrypt the financial data.
