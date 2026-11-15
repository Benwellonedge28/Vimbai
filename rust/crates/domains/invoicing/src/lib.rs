//! *invoicing-service* ([`invoices`]): the customers a Book bills, the invoices it raises, and what has been paid
//! against them, kept on the store and scoped to the caller and the Book.
//!
#![forbid(unsafe_code)]
//! Invoicing services.
//!
//! The invoices and the customers a Book bills are also published on their own addresses - *invoices-service* and
//! *invoice-customers-service* - because a client reaches the wildcard door `/invoicing` by repeating a prefix it did not ask
//! for, while `/invoices` and `/customers` are what its calls are actually written against.
//!
//! * *subscription-plans-service* ([`billing`]): the plans a Book sells on and the subscriptions taken against them.
//!
//! The service keeps its records on the store, scoped to the caller and the Book, and is safe Rust by construction.

pub mod billing;
pub mod invoices;
