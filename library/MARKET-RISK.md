# vimbai_market_risk

Six stateless services, safe Rust, no database and no clock:

| module | service |
| --- | --- |
| `options` | options-pricing (Black-Scholes plus greeks) |
| `var` | market-risk (`/var`) |
| `credit` | credit-risk-analysis (`/analyze`, `/pd-historical`) |
| `fraud` | fraud-risk-assessment |
| `distress` | financial-distress-prediction |
| `regulatory` | regulatory-ratio |

The standard normal is implemented in `normal.rs` (Maclaurin series for erf,
accurate to about 1e-12 for |x| up to 4) so the crate needs nothing beyond serde.

## Defects preserved, each pinned by a test

*VaR ignores correlation.* Portfolio volatility is a weighted average scaled by a
flat 85% "diversification" factor. The `correlation` input is accepted and never
used, and both 95% and 99% VaR are always returned whatever confidence level was
asked for. A genuinely diversified book is over-reported.

*Fraud ignores financial indicators.* `financial_indicators` and `industry` are
accepted and never used, so a request carrying alarming ratios scores exactly the
same as one with none.

*CET1 is measured with Tier 1 capital.* There is no common-equity input, so CET1
and Tier 1 always report the same value against different minimums.

*Credit risk.* Recovery is capped at 90% with a floor of 10% only in the
zero-exposure case, so an unsecured loan recovers nothing. The risk-adjusted
return assumes an 8% margin on the drawn balance, a placeholder rather than a
facility rate. A perfect financial score still only reaches AA, because the
qualitative base of 50 caps the composite.

*Options.* Theta is divided by 365 whatever the time unit, so a per-day decay is
understated when expiry is quoted in days.

*Distress.* Book equity stands in for market value in the fourth Altman term, and
"years to distress" comes from fixed bands rather than a projection.

## Deliberately not ported

`/pd-transition-matrix` needs numpy matrix powers and was not read before the
port; it stays on the Python service rather than being guessed.
