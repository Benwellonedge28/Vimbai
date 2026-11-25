# Payment website
Status: proposed.

## Experience
Public product pages, localized pricing, account sign-in, plan selection, hosted checkout, pending/confirmed status, receipts, subscription management, cancellation and support. Account identity links purchase to existing Vimbai user or organisation; no Book financial data on public pages. Mobile-friendly low-bandwidth design.

## Architecture
Prefer existing repository deployment and shared identity/API contracts; evaluate Flutter web versus dedicated lightweight web surface before creating another frontend, respecting existing single-client ADR. Server creates checkout intent; provider hosts sensitive payment fields; webhook updates billing authority. Website never stores card or mobile-money credentials. Use CSRF protection, secure sessions, rate limits and accessible forms.

## Distribution policy
External website payment and in-app purchase links depend on current platform/store rules, geography and product type. Do not assume Play Store distribution automatically permits external checkout or avoids fees. Verify applicable rules before release; use channel-specific compliant flows.

## Acceptance
Mobile checkout, account mismatch, abandoned session, delayed confirmation, duplicate payment, refund, accessibility, security review and receipts. Publish privacy, terms and cancellation policy before accepting money.
