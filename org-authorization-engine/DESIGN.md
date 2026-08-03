# Org Authorization Engine — Design Document

## Overview

The Org Authorization Engine provides enterprise-grade governance and control for Vimbai. It implements a flexible authorization matrix that combines Role-Based Access Control (RBAC) and Attribute-Based Access Control (ABAC), allowing organizations to map their real-world hierarchies and approval chains directly into Vimbai.

## RBAC + ABAC Model

Organizations do not have identical structures. Vimbai supports dynamic rules:

- **RBAC (Roles):** Defines baseline permissions (e.g., "Auditors can view reports," "Employees can submit expenses").
- **ABAC (Attributes):** Modifies permissions based on context (e.g., Amount, Department, Location, Project).

### Example Workflows Supported

1. **Threshold Approvals:** Expenses > $10,000 require CFO approval.
2. **Boundary Enforcement:** Regional managers can only approve expenses originating from their specific branch.
3. **Multi-Level Chains:** Employee -> Manager -> Director -> CFO.

## Endpoints

| Method | Path | Description |
| :--- | :--- | :--- |
| `GET` | `/health` | Health check |
| `POST` | `/authorize` | Evaluate a requested action against the organization's RBAC/ABAC rules |

## Financial Boundary

Vimbai is a financial management system, not a payment processor. This authorization engine governs the *approval* of budgets and expenses, but **Vimbai never executes the actual transfer of funds**. Money movement remains strictly with the organization's banks and payment providers. This separation of concerns significantly reduces regulatory risk while providing powerful financial intelligence.
