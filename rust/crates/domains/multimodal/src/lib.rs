//! Vimbai multimodal pipeline as a library.
//!
//! *multimodal-pipeline-service* ([`capture`]): the records a Book's inbox holds - a document, image or recording the
//! pipeline has taken in - the per-status counts the inbox badge shows, and the corrections a person made to what was
//! read out of them.//!
//! *tasks-service* ([`capture`]): the same inbox published on `/tasks`, the address the app's capture calls are written against.

#![forbid(unsafe_code)]

pub mod capture;
