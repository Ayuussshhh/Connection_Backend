//! Schema Snapshot Module
//!
//! The heart of SchemaFlow - detecting what changed in the database.
//! This module provides:
//! - Schema snapshots (point-in-time captures)
//! - Schema diff engine (comparing snapshots)
//! - Change detection (what breaks if I change this?)
//! - Blast radius analysis (downstream impact)

pub mod store;
pub mod diff;
pub mod blast_radius;
pub mod rules;

pub use store::SnapshotStore;
#[allow(unused_imports)]
pub use diff::{SchemaDiff, DiffEngine, ChangeType, SchemaDiffItem};
#[allow(unused_imports)]
pub use blast_radius::{BlastRadiusAnalyzer, BlastRadius, ImpactedObject};
#[allow(unused_imports)]
pub use rules::{RulesEngine, Rule, RuleViolation, Severity};
