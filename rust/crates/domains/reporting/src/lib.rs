#![forbid(unsafe_code)]
//! Reporting: the reports a Book asks for and the layouts its figures are read on.
//!
//! Four services live here, and they are here together because they are the four steps of one subject - a report
//! asked for, the schedule that repeats it, the delivery of it, and the board it is read on:
//!
//! * *reporting-service* ([`reports`]): a report asked for and a schedule recorded. It generates no document and
//!   stores no report, which is how it shipped; the answer says so rather than leaving a reader to find out.
//! * *report-automation-service* ([`automation`]): the templates a Book keeps, the schedules they run on, and the
//!   reports those schedules have produced.
//! * *report-distribution-service* ([`distribution`]): the lists a report is addressed to, the subscriptions that
//!   send it, and the record of what was sent.
//! * *dashboard-service* ([`dashboards`]): the four fixed layouts - executive, financial, operations, compliance -
//!   and the custom layout a caller states. Every figure on the fixed layouts is nil, because the shipped service
//!   read nothing to fill them.
//!
//! The arithmetic these two would need is not here. It lives in the reporting calculations crate, and the
//! documents belong to the service that renders them; these two answer shapes.

pub mod automation;
pub mod dashboards;
pub mod distribution;
pub mod reports;
