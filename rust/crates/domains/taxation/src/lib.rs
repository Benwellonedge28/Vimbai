#![forbid(unsafe_code)]
//! The tax domain: *tax-accounting-service* answers from here, with tax-compliance-service and
//! tax-planning-service still to follow into the same crate.
//!
//! One crate, so a rate is read the same way whether a return is being measured or a plan is being argued:
//!
//! * *tax-accounting-service* ([`accounting`]): rates and registration, the tax worked out on a transaction
//!   (single or batch), returns and their filing and payment, withholding tax, the summary and VAT-by-jurisdiction
//!   reports, deferred tax, and the liability schedule.
//!
//! Shared arithmetic lives in [`rates`]: which rate applies and how a rate turns an amount into tax, so the
//! calculation, the withholding entry and the deferred tax provision cannot each apply a percentage differently.
//! Nothing here moves money - a calculation reports the journal it would post, and collecting or paying tax stays
//! the accounting books' business.

pub mod accounting;
pub mod compliance;
pub mod planning;
pub mod rates;
