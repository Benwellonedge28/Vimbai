#![forbid(unsafe_code)]
//! Reporting: the reports a Book asks for and the layouts its figures are read on.
//!
//! Two services live here, and they are here together because they are the two ends of one subject - a report and
//! the board it is read from:
//!
//! * *reporting-service* ([`reports`]): a report asked for and a schedule recorded. It generates no document and
//!   stores no report, which is how it shipped; the answer says so rather than leaving a reader to find out.
//! * *dashboard-service* ([`dashboards`]): the four fixed layouts - executive, financial, operations, compliance -
//!   and the custom layout a caller states. Every figure on the fixed layouts is nil, because the shipped service
//!   read nothing to fill them.
//!
//! The arithmetic these two would need is not here. It lives in the reporting calculations crate, and the
//! documents belong to the service that renders them; these two answer shapes.

pub mod dashboards;
pub mod reports;
