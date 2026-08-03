# Encrypted Backup Service — Design Document

## Overview

The Encrypted Backup Service manages the metadata and integrity verification for Vimbai's user-controlled backup system. Following the "Bring Your Own Storage" (BYOS) principle, Vimbai acts as the encryption engine while users choose where their encrypted `.vmb` files reside (e.g., Google Drive, local storage, private servers).

## The `.vmb` Backup Format

A Vimbai backup is not a simple zip file. It is a cryptographically bound package containing:
- **Encrypted Data:** Transactions, budgets, reports, settings (AES-256-GCM).
- **Backup Metadata:** Version, timestamp, size.
- **Integrity Signature:** To detect tampering.
- **Account Binding Information:** A cryptographic identifier proving the backup belongs to a specific Vimbai identity.

## Core Principles

1. **User Ownership:** The user holds the keys. If the user loses their keys, Vimbai cannot decrypt the data.
2. **Account Binding:** A backup file cannot be imported into a different user's account.
3. **Storage Independence:** Vimbai tracks the metadata, but the user pays for and controls the physical storage of the `.vmb` file.

## Endpoints

| Method | Path | Description |
| :--- | :--- | :--- |
| `GET` | `/health` | Health check |
| `POST` | `/backups/register` | Register metadata for a newly created encrypted backup |
| `POST` | `/backups/verify-binding` | Verify that a backup file belongs to the requesting user before allowing restoration |

## Security & Privacy Alignment

This service ensures that Vimbai never possesses a "master key." By decoupling the encryption process from the storage provider, Vimbai eliminates the risk of a centralized data breach exposing plaintext financial records.
