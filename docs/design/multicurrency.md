# Multicurrency
Status: proposed extension of ADR-012 and common::fx.

## Model
Book functional currency remains singular. Documents retain original currency, exact amount, rate, rate direction, source and timestamp; posted lines use functional currency. Reporting currency is separate. Currency registry supports effective-dated codes and minor units, including historical currencies; no floating-point arithmetic.

## Rules
Rate expresses functional currency per unit of document currency. Reject nonpositive or missing required rates. Settlement differences, period-end restatement and consolidation translation follow ADR-012 canonical rules. Preserve original rate and policy version; corrections use reversal, not historical mutation. Rounding residuals follow configured approved policy.

## Integration
Reuse common Money and fx; accounting orchestrates, treasury supplies approved rate observations, reporting translates, kernel posts. Provider adapters retrieve rates without deciding accounting treatment. Offline rates are explicitly dated and subject to freshness policy.

## Acceptance
Cross-rate direction, exact decimal extremes, partial settlement, returns, restatement, translation, missing rates, historical currency changes and reproducible reports. No implicit conversion or summation across currencies.
