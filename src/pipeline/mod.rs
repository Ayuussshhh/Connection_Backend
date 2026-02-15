//! Governance Pipeline Module
//!
//! The backbone that connects the schema platform to the real world.
//! This implements the four-stage governance pipeline:
//!
//! 1. **Mirror (Introspection)**: Crawl the database, build a semantic map
//! 2. **Proposal (Glow Layer)**: Create draft objects for schema changes  
//! 3. **Brain (Risk Simulation)**: Analyze proposals for safety and impact
//! 4. **Orchestrator (Safe Execution)**: Execute approved changes safely

pub mod mirror;
pub mod proposal;
pub mod risk;
pub mod orchestrator;
pub mod types;
pub mod metadata;

// Re-export main types for convenient access
pub use metadata::MetadataStore;
