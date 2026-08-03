# Vimbai Security & Encryption Whitepaper

## 1. Architecture Overview
Vimbai is an enterprise-grade financial management platform that never touches real money and never reads plaintext financial data on its servers.

## 2. Encryption Standard
All financial data is encrypted on-device using **AES-256-GCM**. 
- **Keys:** Encryption keys are generated locally and protected by the device's secure enclave (e.g., Android Keystore, Apple Secure Enclave).
- **Transmission:** Data is transmitted over TLS 1.3.
- **Storage:** Servers store only the resulting ciphertext, Initialization Vector (IV), and Authentication Tag.

## 3. The `.vmb` Backup Format
Vimbai backups are bound to the user's cryptographic identity. A stolen `.vmb` file cannot be restored by a different Vimbai account.
- Integrity signatures prevent tampering.
- Users can securely store `.vmb` files on Google Drive, AWS, or local USB drives.

## 4. Multi-Factor Authentication (MFA)
Sensitive actions (like restoring a backup or approving a large expense) require MFA. Supported factors include:
- Knowledge: Passwords, PINs, Recovery Phrases
- Biometrics: Fingerprint, Face ID (processed locally via OS APIs; raw data never leaves the device)
- Possession: Trusted devices, Passkeys

## 5. Authorization Engine (RBAC + ABAC)
For organizations, Vimbai supports complex authorization matrices combining Role-Based Access Control (RBAC) and Attribute-Based Access Control (ABAC).
- E.g., "Only the CFO can approve expenses > $10,000."
- E.g., "Department Managers can only view budgets for their assigned location."
