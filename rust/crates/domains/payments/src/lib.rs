//! Vimbai Pay as a library.
//!
//! Vimbai is a record-keeping system and never moves money, so what lives in this crate is the `domain` half of payments:
//! the records a Book keeps about the money its tills take.
//!
//! *pos-integration-service* ([`pos`]): the tills a Book runs, the takings they report, the sales summary those takings
//! add up to, and the outside systems - Square, Stripe, Shopify - whose reports arrive here.

#![forbid(unsafe_code)]

pub mod pos;
