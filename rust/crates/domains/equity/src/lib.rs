//! Equity services: the statement of changes in equity, which records each movement against its Book.
//!
//! Services here:
//! * `equity-changes-service`
//! * `authorized-share-capital-service` (records on the store: share classes, their issuances,
//!   buybacks and the used-capacity summary)
//! * `bonus-shares-service` and `right-issues-service` (records on the store: bonus issues and the
//!   reserve they capitalise, rights offers with their acceptances and renunciations)
//! * `ordinary-shares-service` and `preference-shares-service` (records on the store: dividends,
//!   preference classes, their issuances and redemptions; journal entries are built and returned)
//!
//! One crate per business domain, not one per service: these services were separate Python
//! containers that each carried a copy of the same models, the same database access and the same
//! arithmetic. Grouped, a domain shares one build, one test run and one place to look.

#![forbid(unsafe_code)]

pub mod equity_changes;
pub mod share_classes;
pub mod share_events;
pub mod shares;
pub use share_classes::{
    buyback as buyback_shares, create_class as create_share_class, get_class as get_share_class,
    issue as issue_shares, list_classes as list_share_classes,
    list_issuances as list_share_issuances, summary as share_capital_summary,
};

pub use share_events::{
    accept_rights, bonus_reserve_account, get_right_issue, issue_bonus_shares, list_bonus_issues,
    list_right_issues, offer_rights, renounce_rights,
};
pub use shares::{
    create_preference_class, declare_ordinary_dividend, declare_preference_dividend,
    issue_preference_shares, list_ordinary_dividends, list_preference_classes,
    list_preference_dividends, pay_ordinary_dividend, pay_preference_dividend,
    redeem_preference_shares,
};
