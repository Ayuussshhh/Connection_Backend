//! Pipeline module - Governance workflow components
//!
//! This module provides the legacy governance pipeline infrastructure.
//! The new v2 proposal system is in the `proposal` module.

pub mod metadata;
pub mod mirror;
pub mod orchestrator;
pub mod proposal;
pub mod risk;
pub mod types;

pub use metadata::MetadataStore;
