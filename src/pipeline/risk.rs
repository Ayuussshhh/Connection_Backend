//! Risk analysis engine

use crate::error::AppError;
use crate::pipeline::proposal::{RiskAnalysis, RiskLevel, SchemaProposal};
use crate::pipeline::types::SchemaChange;
use chrono::Utc;

/// Risk analysis engine
pub struct RiskEngine;

impl RiskEngine {
    pub fn new() -> Self {
        Self
    }

    /// Analyze the risk of a proposal
    pub fn analyze(&self, proposal: &SchemaProposal) -> Result<RiskAnalysis, AppError> {
        let mut score = 0u32;
        let mut warnings = Vec::new();
        let mut recommendations = Vec::new();
        let mut affected_tables = Vec::new();
        let mut requires_downtime = false;

        for change in &proposal.changes {
            match change {
                SchemaChange::DropTable { table_name } => {
                    score += 100;
                    warnings.push(format!("Dropping table '{}' is destructive and irreversible", table_name));
                    affected_tables.push(table_name.clone());
                    requires_downtime = true;
                }
                SchemaChange::DropColumn { table_name, column_name } => {
                    score += 50;
                    warnings.push(format!("Dropping column '{}' from '{}' is destructive", column_name, table_name));
                    affected_tables.push(table_name.clone());
                }
                SchemaChange::AlterColumn { table_name, column_name, new_type, .. } => {
                    if new_type.is_some() {
                        score += 30;
                        warnings.push(format!("Changing type of '{}' in '{}' may cause data loss", column_name, table_name));
                    }
                    affected_tables.push(table_name.clone());
                }
                SchemaChange::CreateTable { table_name, .. } => {
                    score += 5;
                    affected_tables.push(table_name.clone());
                }
                SchemaChange::AddColumn { table_name, column, .. } => {
                    if !column.nullable && column.default_value.is_none() {
                        score += 20;
                        warnings.push(format!("Adding non-nullable column '{}' without default to '{}' may fail on existing rows", column.name, table_name));
                    }
                    affected_tables.push(table_name.clone());
                }
                SchemaChange::AddIndex { table_name, .. } => {
                    score += 10;
                    recommendations.push(format!("Consider using CONCURRENTLY for index on '{}'", table_name));
                    affected_tables.push(table_name.clone());
                }
                SchemaChange::AddForeignKey { table_name, .. } => {
                    score += 15;
                    affected_tables.push(table_name.clone());
                }
                _ => {
                    score += 5;
                }
            }
        }

        // Deduplicate affected tables
        affected_tables.sort();
        affected_tables.dedup();

        let overall_risk = match score {
            0..=20 => RiskLevel::Low,
            21..=50 => RiskLevel::Medium,
            51..=100 => RiskLevel::High,
            _ => RiskLevel::Critical,
        };

        if score > 50 {
            recommendations.push("Consider testing this migration on a staging environment first".to_string());
        }
        if score > 100 {
            recommendations.push("Schedule this migration during a maintenance window".to_string());
        }

        Ok(RiskAnalysis {
            overall_risk,
            score,
            warnings,
            recommendations,
            estimated_duration_secs: (score as u64 / 10).max(1),
            requires_downtime,
            affected_tables,
            analyzed_at: Utc::now(),
        })
    }
}

impl Default for RiskEngine {
    fn default() -> Self {
        Self::new()
    }
}
