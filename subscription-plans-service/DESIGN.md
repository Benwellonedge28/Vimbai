# Subscription Plans Service — Design Document

## Overview

The Subscription Plans Service is the authoritative source of truth for all Vimbai pricing tiers and their associated feature sets. It exposes a read-only catalogue of the seven plans available on the platform, enabling other services (billing, onboarding, feature-flag enforcement) to query plan definitions without hardcoding them.

## Plan Catalogue

| Plan | Target Users | Monthly Price |
| :--- | :--- | :---: |
| **Free** | Individual users getting started | $0.00 |
| **Family** | Families or small groups (2–10 users) | $9.99 |
| **Basic** | Sole traders and small businesses | $19.99 |
| **Professional** | Growing businesses | $39.99 |
| **Business** | Medium-sized companies | $99.99 |
| **Enterprise** | Large organizations | $299.99 |
| **Government** | Public sector organizations | $499.99 |

## Architecture

The service is implemented as a stateless FastAPI microservice. Plan definitions are stored as in-memory Pydantic models at startup, making reads extremely fast with zero database latency. For production, plan definitions should be loaded from a configuration store (e.g., a managed key-value store) to allow pricing changes without redeployment.

## Endpoints

| Method | Path | Description |
| :--- | :--- | :--- |
| `GET` | `/health` | Health check |
| `GET` | `/plans` | List all subscription plans |
| `GET` | `/plans/{plan_id}` | Retrieve a specific plan by ID |

## Security

This service is read-only and does not handle payment processing. All write operations (subscription creation, upgrades, cancellations) are delegated to the billing service. No authentication is required to list plans, as the catalogue is public information.

## Privacy Alignment

The service contains no user data. It stores only product definitions and is therefore fully compliant with the Vimbai zero-trust privacy model.
