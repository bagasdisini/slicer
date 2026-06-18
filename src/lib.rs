//! Slicer — a streaming log parser and analyzer library.
//!
//! This crate provides the core parsing engine, data models, and output
//! rendering for the `slicer` CLI tool.  It is structured for both
//! library consumption and binary entry-point use.

pub mod cli;
pub mod models;
pub mod output;
pub mod parser;
