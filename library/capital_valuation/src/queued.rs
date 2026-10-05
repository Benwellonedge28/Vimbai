#![forbid(unsafe_code)]
//! Formula inventory for the remaining capital/valuation services queued
//! to port into this crate. Each entry lists the source service and the
//! formulas its Python implementation uses, so the ports stay mechanical.
//!
//! 1. accounting-rate-return-service — ARR = avg annual profit / initial
//!    investment (+ total profit and per-year variants).
//! 2. cost-of-capital-service — WACC = (E/V)*Re + (D/V)*Rd*(1-t), CAPM
//!    Re = Rf + beta*(Rm - Rf), cost of debt and retained-earnings variants.
//! 3. business-valuation-service / merger-valuation-service — DCF
//!    valuation, goodwill, relative valuation multiples.
//! 4. eva-service — EVA = NOPAT - (capital invested * WACC).
//! 5. croic-service — CROIC = NOPAT / invested capital.
//! 6. mva-service — MVA = market value - invested capital.
//! 7. du-pont-service — 3/5-step DuPont decomposition of ROE.
//! 8. asset-allocation-service / portfolio-optimization-service —
//!    expected return and variance of a portfolio, weights.
//! 9. risk-return-analysis-service — std dev, coefficient of variation,
//!    Sharpe ratio.
//! 10. sustainable-growth-service — g = ROE * retention ratio.
//! 11. initial-investment-service / investment-appraisal-service /
//!     capital-budgeting-service — funding breakdowns and accept/reject
//!     aggregation over NPV/IRR/payback/PI.
//! 12. deal-structuring-service / divestiture-service / post-merger-service /
//!     synergy-analysis-service — payment structures, gain/loss on
//!     disposal, combined-entity computations.
//! 13. asset-turnover-service — revenue / average total assets.
//!
//! Before each port: read the Python service, port formulas + serde
//! models, and add parity tests with values verified against the
//! original service.
