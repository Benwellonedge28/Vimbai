//! Equity services: the statement of changes in equity, which records each movement against its Book.
//!
//! Services here:
//! * `equity-changes-service`
//! * `authorized-share-capital-service` (records on the store: share classes, their issuances,
//!   buybacks and the used-capacity summary)
//! * `debentures-service` (records on the store: classes, issues, per-period interest with a flat
//!   twenty percent withheld, and redemptions with their premium)
//! * `issued-share-capital-service` (records on the store: issues at the hard-coded one-unit nominal,
//!   the shareholder register, and the capital summary)
//! * `initial-investment-service` (records on the store, plus the simple outlay and capital allowance
//!   arithmetic that stores nothing)
//! * `share-redemption-service` (records on the store: redemptions through initiation, a fresh issue
//!   that funds one, and completion, with the reserve requirement between them)
//! * `share-premium-service` (records on the store: premium entries, utilizations and signed
//!   adjustments, with the balance the three of them share)
//! * `capital-redemption-reserve-service` (records on the store: redemptions that create the reserve,
//!   reserves created by a capital reduction, and utilizations)
//! * `general-reserve-service` (records on the store: reserves with their allocations and
//!   utilizations, the balance the only figure they share)
//! * `retained-profits-service` (records on the store: appropriations with their carry-forward chain,
//!   interim dividends and prior-year adjustments)
//! * `bonus-shares-service` and `right-issues-service` (records on the store: bonus issues and the
//!   reserve they capitalise, rights offers with their acceptances and renunciations)
//! * `ordinary-shares-service` and `preference-shares-service` (records on the store: dividends,
//!   preference classes, their issuances and redemptions; journal entries are built and returned)
//!
//! One crate per business domain, not one per service: these services were separate Python
//! containers that each carried a copy of the same models, the same database access and the same
//! arithmetic. Grouped, a domain shares one build, one test run and one place to look.

#![forbid(unsafe_code)]

pub mod debentures;
pub mod equity_changes;
pub mod initial_investment;
pub mod issued_share_capital;
pub mod redemption_reserve;
pub mod reserves;
pub mod retained_profits;
pub mod share_classes;
pub mod share_events;
pub mod share_premium;
pub mod share_redemption;
pub mod shares;
pub use share_classes::{
    buyback as buyback_shares, create_class as create_share_class, get_class as get_share_class,
    issue as issue_shares, list_classes as list_share_classes,
    list_issuances as list_share_issuances, summary as share_capital_summary,
};

pub use debentures::{
    accrue_interest, create_class as create_debenture_class, issue as issue_debentures,
    list_classes as list_debenture_classes, list_interest as list_debenture_interest,
    list_issues as list_debenture_issues, pay_interest, period_interest, periods_per_year, redeem,
    split_tax,
};
pub use initial_investment::{
    capital_allowance_saving, create_investment, get_investment, list_investments, simple_outlay,
};
pub use issued_share_capital::{
    capital_summary, issue_capital, issue_proceeds, list_issues, list_shareholders, record_issue,
    register_shareholder,
};
pub use redemption_reserve::{
    capital_account, create_crr, crr_summary, list_creations as list_crr_creations,
    list_redemptions as list_crr_redemptions, list_utilizations as list_crr_utilizations,
    record_redemption, redemption_proceeds, redemption_reserve,
    utilization_destination as crr_utilization_destination, utilize_crr,
};
pub use reserves::{
    allocate_to_reserve, allocation_source_account, create_reserve, get_reserve, list_reserves,
    reserve_history, utilization_destination_account, utilize_reserve,
};
pub use retained_profits::{
    appropriation_totals, create_appropriation, create_prior_year_adjustment,
    declare_interim_dividend, list_adjustments, list_appropriations, list_interim_dividends,
    pay_interim_dividend, retained_earnings_summary,
};
pub use share_events::{
    accept_rights, bonus_reserve_account, get_right_issue, issue_bonus_shares, list_bonus_issues,
    list_right_issues, offer_rights, renounce_rights,
};
pub use share_premium::{
    adjust_premium, list_adjustments as list_premium_adjustments,
    list_entries as list_premium_entries, list_utilizations as list_premium_utilizations,
    premium_on_issue, premium_summary, record_share_premium, utilization_destination,
    utilize_premium,
};
pub use share_redemption::{
    capital_account as redemption_capital_account, complete_redemption, initiate_redemption,
    list_crr_requirements, list_redemptions as list_share_redemptions, record_fresh_issue,
    redemption_detail, reserve_created, total_value as redemption_total_value,
};
pub use shares::{
    create_preference_class, declare_ordinary_dividend, declare_preference_dividend,
    issue_preference_shares, list_ordinary_dividends, list_preference_classes,
    list_preference_dividends, pay_ordinary_dividend, pay_preference_dividend,
    redeem_preference_shares,
};
