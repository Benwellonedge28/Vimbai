#![forbid(unsafe_code)]
//! Variance analysis library.
//!
//! Consolidates the nine original variance microservices into one
//! pure-Rust library, preserving each service's exact formulas and
//! JSON response shapes:
//!
//! 1. `labour` — labour-cost-variance-service, labour-efficiency-variance-service,
//!    labour-rate-variance-service, variance-service `/labour-variance`
//! 2. `material` — material-cost-variance-service, material-price-variance-service,
//!    material-usage-variance-service, variance-service `/material-variance`
//! 3. `sales` — sales-price-variance-service, sales-volume-variance-service,
//!    variance-service `/sales-variance`
//! 4. `generic` — variance-service `/calculate` (budgeted vs actual)

pub mod generic;
pub mod labour;
pub mod material;
pub mod sales;

pub use generic::{cost_variance, revenue_variance, VarianceKind};
pub use labour::{department_labour_analysis, labour_rate_variance, labour_variance};
pub use material::{
    material_cost_variance, material_efficiency_variance, material_price_variance,
    material_total_variance, material_usage_variance, multi_material_variance,
};
pub use sales::{sales_price_volume_analysis, sales_volume_cm_analysis};
