#![forbid(unsafe_code)]
//! Non-profit accounting: one crate for the whole non-profit domain.
//!
//! The two non-profit services used to be separate containers that each carried their own copy of the same fund
//! arithmetic. They live here together, so a fund's balance is worked out one way whether a small charity or a
//! multi-branch non-profit is asking:
//!
//! * *npo-service* ([`npo`]): funds and their restrictions, donations, grants and drawdowns, donors, projects and
//!   programs, budgets, governance records, impact measurement, and the two statements;
//! * *npo-scale-service* ([`npo_scale`]): the same non-profit at scale - organisations, their branches and their
//!   books, creditors, payroll-side records and the consolidations that report them.
//!
//! A fund balance is never taken from a caller: it moves by the transactions recorded against the fund, which is the
//! rule both services are built on. No money moves here - Vimbai keeps records.

pub mod npo;
pub mod npo_scale;
