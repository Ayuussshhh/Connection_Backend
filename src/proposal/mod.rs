//! Proposal module - The heart of the governance pipeline
//!
//! Handles schema change proposals, reviews, and approvals.

mod models;
mod store;
mod changes;
mod migration;

pub use models::*;
pub use store::ProposalStore;
pub use changes::*;
pub use migration::MigrationGenerator;
