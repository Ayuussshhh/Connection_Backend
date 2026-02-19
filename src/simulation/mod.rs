//! Simulation engine for risk analysis
//!
//! Analyzes schema changes to predict execution time, locks, and downstream impacts.

mod analyzer;
mod dry_run;

#[allow(unused_imports)]
pub use analyzer::RiskAnalyzer;
#[allow(unused_imports)]
pub use dry_run::DryRunner;
