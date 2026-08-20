//! Language Server Protocol implementation for the Kamailio
//! configuration file language (`kamailio.cfg`).
//!
//! Semantic truth stays in Kamailio itself: diagnostics come from a
//! real `kamailio -c` run; completion/hover come from a documentation
//! catalog harvested out of a Kamailio source tree (module READMEs)
//! and a kamailio-wiki checkout (core cookbooks).
#![deny(missing_docs)]

/// Lexical analysis of cfg documents.
pub mod analyze;
/// Module documentation harvesting.
pub mod catalog;
/// The `check` CLI subcommand.
pub mod cli;
/// `kamailio -c` diagnostics.
pub mod diag;
/// Completion/hover/definition assembly.
pub mod logic;
/// The tower-lsp server wiring.
pub mod server;
