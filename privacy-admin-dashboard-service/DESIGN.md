# Privacy-Preserving Admin Dashboard Service — Design Document

## Overview

The Privacy-Preserving Admin Dashboard Service provides Vimbai operators with operational visibility into platform health and growth — without exposing any user financial data. It is the enforcement layer for the principle that administrators should see only what is necessary to run the platform.

## Permitted vs Forbidden Metrics

The service enforces a strict boundary between operational telemetry and personal financial data.

| Permitted (Operational) | Forbidden (Personal Finance) |
| :--- | :--- |
| Number of registered users | "Samuel spent $500 on groceries" |
| Active users (monthly/weekly) | "Company X has $1 million revenue" |
| New signups | Any individual transaction detail |
| Subscription conversions | Any user's income or expense |
| App crashes and errors | Any company's financial report |
| Storage usage | Any private document content |
| Feature usage counts (aggregated) | |
| Average app startup time | |

## Architecture

The service is a FastAPI microservice that aggregates anonymised, aggregated telemetry from the platform's observability stack (e.g., Prometheus, Grafana). It never queries user financial databases. Feature usage counts are collected using privacy-preserving counting techniques (differential privacy or simple aggregation without user-level attribution).

## Endpoints

| Method | Path | Description |
| :--- | :--- | :--- |
| `GET` | `/health` | Health check |
| `GET` | `/metrics/operational` | Retrieve operational metrics (admin only) |

## Security

All endpoints are protected by admin-level authentication. Standard user tokens are rejected with HTTP 403. Admin tokens are short-lived, audited, and issued only to authorised platform operators.

## Privacy Alignment

This service is the operational expression of Vimbai's privacy commitment. It demonstrates that a platform can be fully observable from an operational standpoint while remaining completely blind to user financial details.
