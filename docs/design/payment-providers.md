# Subscription payment providers
Status: proposed; ADR-005 remains binding for customer money movement.

## Boundary
Vimbai collects payment for its own software through authorized external merchant providers. This is not Vimbai Pay, a wallet, settlement product or customer payment gateway. Existing domains/payments is POS recordkeeping and must not be repurposed. Obtain merchant agreement and regulatory review before implementation; amend ADR if scope conflicts.

## Provider contract
Create hosted checkout, query transaction, verify webhook, retrieve settlement report and request refund where contract permits. Capability registry declares supported currencies, channels, recurring consent, refunds and availability. Candidate Zimbabwe integration: Paynow and EcoCash where merchant contract and actual supported methods permit; no assumption all channels or recurring payments are available.

## Security
Secrets server-side, signed webhook verification, replay protection, unique provider transaction ID, amount/currency/order match, redacted logs. Unknown status remains pending; never grant access from screenshot or redirect. Reconcile gross, fees, net and settlement date.

## Integration
Commercial context owns invoices and subscriptions; external merchant adapter owns provider protocol; worker reconciles; existing accounting records Vimbai's own revenue. No customer funds held. Test sandbox then controlled live transaction, timeout, forged callback, duplicate callback, refund and mismatch.
