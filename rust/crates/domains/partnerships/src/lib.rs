#![forbid(unsafe_code)]
//! Partnerships: the services that keep a partnership's register and the changes made to it.
//!
//! Seven services live here, together because they answer questions about the same register:
//!
//! * *partnership-agreement-service* ([`agreement`]): agreements, their partners and the capital they add up to;
//! * *partnership-changes-service* ([`changes`]): admission, retirement, death and the settlement of what a
//!   departing partner is owed;
//! * *partnership-dissolution-service* ([`dissolution`]): what is realised on dissolution and who gets what;
//! * *partnership-revaluation-service* ([`revaluation`]): asset revaluations and how goodwill is treated;
//! * *family-community-group-service* ([`groups`]): savings groups, their members and the pool they collect;
//! * *npo-service* ([`npo`]): a non-profit's funds, donations, grants, projects and the statements that report them;
//! * *npo-scale-service* ([`npo_scale`]): the multi-entity non-profit, with branches, books and consolidations.
//!
//! Shared arithmetic lives in [`figures`] so the four services cannot drift apart, and every record is held through
//! the store, scoped to the caller's Book. No service here moves money: the journal a change implies is drafted and
//! kept with the record, and posting it is the accounting books' business.

pub mod agreement;
pub mod changes;
pub mod dissolution;
pub mod figures;
pub mod groups;
pub mod revaluation;
