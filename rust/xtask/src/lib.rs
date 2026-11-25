//! Repository tooling that outlived the Python scripts it replaced.
//!
//! The binary and the tests call the same functions, so what a gate checks is what the generator writes: a generator
//! tested through a shell invocation is a generator whose output nobody asserts.
#![forbid(unsafe_code)]

pub mod app_calls;
pub mod boundaries;
pub mod business_words;
pub mod catalog;
pub mod crates;
pub mod deploy;
pub mod documentation;
pub mod domains;
pub mod flutter;
pub mod gateway;
pub mod images;
pub mod makefile;
pub mod manifests;
pub mod offline_schema;
pub mod openapi;
pub mod owners;
pub mod preflight;
pub mod published;
pub mod reachability;
pub mod readme;
pub mod repository;
pub mod runbook;
pub mod safety;
pub mod servers;
pub mod settings;
pub mod single_source;
pub mod vocabulary;
pub mod wiring;
pub mod workflows;
pub mod workspace;
