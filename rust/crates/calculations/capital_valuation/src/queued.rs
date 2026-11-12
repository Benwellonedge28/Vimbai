#![forbid(unsafe_code)]
//! Formula inventory for the remaining capital/valuation services queued
//! to port into this crate. Each entry lists the source service and the
//! formulas its Python implementation uses, so the ports stay mechanical.
//!
//! 1. accounting-rate-return-service — ARR = avg annual profit / initial
//!    investment (+ total profit and per-year variants).
//! 2. cost-of-capital-service — WACC = (E/V)*Re + (D/V)*Rd*(1-t), CAPM
//!    Re = Rf + beta*(Rm - Rf), cost of debt and retained-earnings variants.
//! 4. eva-service — EVA = NOPAT - (capital invested * WACC).
//! 5. croic-service — CROIC = NOPAT / invested capital.
//! 6. du-pont-service — 3/5-step DuPont decomposition of ROE.
//! 9. asset-turnover-service — revenue / average total assets.
//!
//! Ported and removed from this list (see growth.rs, allocation.rs,
//! appraisal.rs and time_value.rs): mva, sustainable-growth, asset-allocation,
//! portfolio-optimization, capital-allocation, investment-appraisal,
//! project-appraisal, time-value-of-money, present-value.
//!
//! Second batch, now in risk_return.rs, valuation.rs and merger.rs:
//! risk-return-analysis, business-valuation and merger-valuation.
//!
//! Third batch, now in corporate_events.rs: deal-structuring, divestiture,
//! post-merger and synergy-analysis. With those the corporate events and
//! valuation families are fully ported. What is left here is whatever the
//! investor-metric and capital-return families have not already taken
//! (accounting-rate-return, cost-of-capital, EVA, CROIC, du-Pont,
//! asset-turnover), which are ported in ratios_statements.
//!
//! Before each port: read the Python service, port formulas + serde
//! models, and add parity tests with values verified against the
//! original service.
