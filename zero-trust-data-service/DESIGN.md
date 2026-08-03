# Zero-Trust Data Service — Design Document

## Overview

The Zero-Trust Data Service implements Vimbai's privacy-by-design architecture for cloud synchronisation. It acts as a secure, opaque relay for encrypted user data blobs. The server stores and retrieves data without ever being able to read its contents. All encryption and decryption occurs exclusively on the user's device.

## Privacy Model

This service embodies the principle that **the server should not be able to read user financial data**. The following data categories are explicitly forbidden from being stored in plaintext:

- Income details
- Expenses
- Financial reports
- Private documents

The server stores only what it needs to operate:

| Stored on Server | Not Stored on Server |
| :--- | :--- |
| Account identity (user_id) | Income details |
| Subscription status | Expenses |
| Authentication information | Financial reports |
| Encrypted data blobs (opaque) | Private documents |

## Encryption Architecture

Data is encrypted on-device using AES-256-GCM before transmission. The encrypted payload, initialisation vector (IV), and authentication tag are transmitted together as a blob. The server stores the blob atomically and returns it verbatim on pull requests. The device holds the decryption keys, protected by Android Keystore or equivalent platform security features.

## Endpoints

| Method | Path | Description |
| :--- | :--- | :--- |
| `GET` | `/health` | Health check |
| `POST` | `/sync/push` | Upload an encrypted data blob for cloud sync |
| `GET` | `/sync/pull/{user_id}` | Retrieve all encrypted blobs for a user |

## Security

All endpoints require a valid Bearer token in the `Authorization` header. The service validates the token against the identity service but never inspects the payload of the encrypted blob. TLS is enforced for all transport.

## Privacy Alignment

This service is the technical foundation of Vimbai's strongest selling point: true end-to-end encryption where even the platform operator cannot read user financial data.
