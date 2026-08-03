# Enterprise SSO Service — Design Document

## Overview

The Enterprise SSO (Single Sign-On) Service enables large organisations on the Enterprise and Government plans to authenticate their users through their own Identity Provider (IdP). This eliminates the need for Vimbai to store or manage enterprise user credentials directly, reducing the platform's data exposure surface.

## Design Principle

> Users authenticate through their organisation's identity provider. Vimbai receives authentication confirmation, not unnecessary personal information.

The service validates the IdP-issued token and issues a Vimbai-scoped access token. It does not store, log, or forward any personal attributes beyond what is strictly required for session management.

## Supported Identity Providers

The service is designed to be IdP-agnostic via the OIDC/SAML standard. Common integrations include:

| Provider | Protocol |
| :--- | :--- |
| Okta | OIDC / SAML 2.0 |
| Microsoft Azure Active Directory | OIDC / SAML 2.0 |
| Google Workspace | OIDC |
| Custom Enterprise IdP | SAML 2.0 |

## Authentication Flow

1. The user authenticates with their organisation's IdP.
2. The IdP issues a signed token to the client application.
3. The client sends the IdP token to this service's `/auth/sso` endpoint.
4. The service validates the token signature against the IdP's published public keys.
5. Upon successful validation, the service issues a Vimbai JWT access token.
6. The client uses the Vimbai token for all subsequent API calls.

## Endpoints

| Method | Path | Description |
| :--- | :--- | :--- |
| `GET` | `/health` | Health check |
| `POST` | `/auth/sso` | Validate an IdP token and receive a Vimbai access token |

## Data Minimisation

The service explicitly avoids requesting or storing: email addresses, phone numbers, physical addresses, dates of birth, or any financial attributes from the IdP. Only the organisation ID and a session identifier are retained for the duration of the session.

## Privacy Alignment

SSO is a key privacy feature for Enterprise and Government customers. It keeps user identity management within the organisation's own security perimeter, consistent with Vimbai's zero-trust privacy model.
