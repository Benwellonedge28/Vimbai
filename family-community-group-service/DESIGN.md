# Family & Community Group Service — Design Document

## Overview

The Family & Community Group Service extends Vimbai's reach beyond individual and business users to serve families, clubs, churches, sports teams, and community organisations. It provides shared financial management tools tailored to the collaborative nature of these groups, while maintaining the platform's strict privacy model through role-based access controls.

## Supported Group Types

| Group Type | Typical Use Case |
| :--- | :--- |
| **Family** | Shared household budgets, bill reminders, expense splitting between family members |
| **Club** | Member contribution collection, shared expense tracking, committee-level reporting |
| **Church** | Tithe and offering collection, ministry budgets, annual financial reports |
| **Sports Team** | Subscription fee collection, kit and travel expense management |
| **Community** | Neighbourhood association budgets, event expense tracking |

## Key Features

The service provides the following capabilities, all scoped to the Family subscription plan and above:

**For Families:** Up to 10 members per group, shared household budgets, shared savings goals, bill reminders, expense splitting, shared document storage for receipts and warranties, and role-based permissions (e.g., parents manage budgets while children have limited view-only access).

**For Community Groups:** Member contribution collection, shared expense tracking, budget creation, financial report generation, and the ability to assign specialised roles such as Treasurer and Auditor.

## Role-Based Permission Model

| Role | Permissions |
| :--- | :--- |
| **Parent / Admin** | Full read/write access to all group finances |
| **Treasurer** | Manage contributions, expenses, and reports |
| **Auditor** | Read-only access to all financial records |
| **Member** | View shared budgets; submit own expenses |
| **Child** | View-only access to assigned budget categories |

## Endpoints

| Method | Path | Description |
| :--- | :--- | :--- |
| `GET` | `/health` | Health check |
| `POST` | `/groups` | Create a new family or community group |
| `GET` | `/groups/{group_id}` | Retrieve group details and members |
| `POST` | `/groups/{group_id}/contributions` | Record a member contribution |
| `POST` | `/groups/{group_id}/expenses/split` | Split an expense among group members |

## Privacy Alignment

All group financial data is encrypted on-device before being synced to the server via the Zero-Trust Data Service. The server stores only encrypted blobs and group membership metadata. Individual members' financial contributions and spending are never visible to the platform operator.
