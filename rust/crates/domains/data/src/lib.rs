#![forbid(unsafe_code)]
//! The Book's data shelf.
//!
//! * *data-warehouse-service* ([`warehouse`]): the dimension and fact tables a Book describes, the aggregate queries it
//!   runs against them, and the loads that filled them.
//! * encrypted-backup-service ([`backup`]): the backups a Book took, the key each was sealed with, and the restores asked of them.
//! * *etl-service* ([`pipelines`]): the pipelines a Book defines, their steps, and the runs that walked them.
//!
//! Both are kept together because they answer one question - what does this Book hold, and how did it get here - and a
//! warehouse filled by a pipeline is a single story told in two halves.

pub mod backup;
pub mod pipelines;
pub mod warehouse;
