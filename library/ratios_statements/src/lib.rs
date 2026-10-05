#![forbid(unsafe_code)]
//! Ratio and statement generation math: DuPont components, liquidity/efficiency/coverage ratios, consolidated statements, working capital cycles
//!
//! Scaffold for the $name calculation library. The domain map below
//! lists the Python services to port here; port them following the
//! established pattern (see `variance` and `capital_valuation`):
//! read the Python service, port formulas + serde models, add parity
//! tests with values verified against the original.
