//! Simulation engine for risk analysis
//!
//! Analyzes schema changes to predict execution time, locks, and downstream impacts.

mod analyzer;
mod dry_run;

pub use analyzer::RiskAnalyzer;
pub use dry_run::DryRunner;
