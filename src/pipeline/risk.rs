//! Stage Three: The Brain (Risk Simulation)
//!
//! The "Billion-Dollar Stage" - analyzes proposals and generates safety scores.
//! This module performs "dry run" analysis to determine:
//! - Table sizes and impact
//! - Locking behavior and duration
//! - Downstream dependencies
//! - Overall safety score

use crate::error::AppError;
use crate::pipeline::mirror::SemanticMap;
use crate::pipeline::proposal::{
    AffectedTableRisk, DownstreamImpact, ImpactType, MigrationArtifacts,
    Recommendation, RecommendationPriority, RiskAnalysis, RiskFactor,
    RiskLevel, RiskSeverity, SchemaProposal,
};
use crate::pipeline::types::*;
use chrono::Utc;
use std::collections::HashMap;
use tracing::info;

// =============================================================================
// RISK ENGINE - The Brain
// =============================================================================

/// The Brain - analyzes proposals and generates safety scores
pub struct RiskEngine;

impl RiskEngine {
    /// Run a comprehensive risk analysis on a proposal
    pub async fn analyze(
        proposal: &SchemaProposal,
        semantic_map: &SemanticMap,
        _migration: &MigrationArtifacts,
    ) -> Result<RiskAnalysis, AppError> {
        info!("🧠 Running risk analysis for proposal: {}", proposal.title);
        
        let mut risk_factors = Vec::new();
        let mut recommendations = Vec::new();
        
        // 1. Analyze affected tables
        let affected_tables = Self::analyze_affected_tables(proposal, semantic_map)?;
        
        // 2. Analyze downstream impacts
        let downstream_impacts = Self::analyze_downstream_impacts(proposal, semantic_map)?;
        
        // 3. Calculate individual risk factors
        risk_factors.extend(Self::calculate_size_risk(&affected_tables));
        risk_factors.extend(Self::calculate_lock_risk(&affected_tables, proposal));
        risk_factors.extend(Self::calculate_hotspot_risk(&affected_tables, semantic_map));
        risk_factors.extend(Self::calculate_dependency_risk(&downstream_impacts));
        risk_factors.extend(Self::calculate_destructive_risk(proposal));
        risk_factors.extend(Self::calculate_pii_risk(proposal, semantic_map));
        
        // 4. Calculate overall safety score (100 = perfectly safe, 0 = extremely risky)
        let safety_score = Self::calculate_safety_score(&risk_factors);
        let risk_level = RiskLevel::from_score(safety_score);
        
        // 5. Estimate lock duration
        let estimated_lock_duration_ms = Self::estimate_lock_duration(&affected_tables, proposal);
        
        // 6. Generate recommendations
        recommendations.extend(Self::generate_recommendations(
            &risk_factors,
            &affected_tables,
            &downstream_impacts,
            estimated_lock_duration_ms,
        ));
        
        info!(
            "🧠 Risk analysis complete: score={:.1}, level={:?}, factors={}",
            safety_score, risk_level, risk_factors.len()
        );
        
        Ok(RiskAnalysis {
            safety_score,
            risk_level,
            estimated_lock_duration_ms,
            affected_tables,
            downstream_impacts,
            risk_factors,
            recommendations,
            analyzed_at: Utc::now(),
        })
    }
    
    /// Analyze which tables are affected and their risk profiles
    fn analyze_affected_tables(
        proposal: &SchemaProposal,
        semantic_map: &SemanticMap,
    ) -> Result<Vec<AffectedTableRisk>, AppError> {
        let mut affected = HashMap::new();
        
        for change in &proposal.changes {
            let table_key = Self::get_table_key_from_change(change);
            if let Some(key) = table_key {
                if !affected.contains_key(&key) {
                    let (schema, table_name) = Self::split_table_key(&key);
                    
                    // Get stats from semantic map
                    let stats = semantic_map.statistics.get(&key);
                    let is_hot_spot = semantic_map.hot_spots.iter()
                        .any(|hs| hs.schema == schema && hs.table_name == table_name);
                    
                    let (size_category, row_count, size_bytes) = if let Some(s) = stats {
                        (s.size_category(), s.row_count, s.total_size_bytes)
                    } else {
                        (SizeCategory::Small, 0, 0)
                    };
                    
                    let lock_required = Self::change_requires_lock(change);
                    let estimated_lock_ms = if lock_required {
                        Some(Self::estimate_single_table_lock(row_count, size_bytes, change))
                    } else {
                        None
                    };
                    
                    affected.insert(key.clone(), AffectedTableRisk {
                        schema,
                        table_name,
                        size_category,
                        row_count,
                        size_bytes,
                        lock_required,
                        estimated_lock_ms,
                        is_hot_spot,
                    });
                }
            }
        }
        
        Ok(affected.into_values().collect())
    }
    
    /// Analyze downstream impacts on dependent objects
    fn analyze_downstream_impacts(
        proposal: &SchemaProposal,
        semantic_map: &SemanticMap,
    ) -> Result<Vec<DownstreamImpact>, AppError> {
        let mut impacts = Vec::new();
        
        for change in &proposal.changes {
            if let Some(table_key) = Self::get_table_key_from_change(change) {
                if let Some(deps) = semantic_map.dependencies.get(&table_key) {
                    for dep in deps {
                        let impact_type = match (&change, &dep.dependency_type) {
                            (SchemaChange::DropTable(_), _) => ImpactType::Breaking,
                            (SchemaChange::DropColumn(_), DependencyType::Hard) => ImpactType::Breaking,
                            (SchemaChange::RenameTable(_), DependencyType::Hard) => ImpactType::RequiresUpdate,
                            (SchemaChange::RenameColumn(_), DependencyType::Hard) => ImpactType::RequiresUpdate,
                            (SchemaChange::AlterColumn(_), DependencyType::Hard) => ImpactType::PotentialIssue,
                            _ => ImpactType::Informational,
                        };
                        
                        let description = match impact_type {
                            ImpactType::Breaking => format!(
                                "{} '{}' will BREAK if this change is applied",
                                format!("{:?}", dep.object_type).to_lowercase(),
                                dep.name
                            ),
                            ImpactType::RequiresUpdate => format!(
                                "{} '{}' will need to be updated after this change",
                                format!("{:?}", dep.object_type).to_lowercase(),
                                dep.name
                            ),
                            ImpactType::PotentialIssue => format!(
                                "{} '{}' may be affected by this change",
                                format!("{:?}", dep.object_type).to_lowercase(),
                                dep.name
                            ),
                            ImpactType::Informational => format!(
                                "{} '{}' depends on this table",
                                format!("{:?}", dep.object_type).to_lowercase(),
                                dep.name
                            ),
                        };
                        
                        impacts.push(DownstreamImpact {
                            object_type: dep.object_type,
                            schema: dep.schema.clone(),
                            name: dep.name.clone(),
                            impact_type,
                            description,
                        });
                    }
                }
            }
        }
        
        Ok(impacts)
    }
    
    /// Calculate risk factors related to table size
    fn calculate_size_risk(affected_tables: &[AffectedTableRisk]) -> Vec<RiskFactor> {
        let mut factors = Vec::new();
        
        for table in affected_tables {
            match table.size_category {
                SizeCategory::VeryLarge => {
                    factors.push(RiskFactor {
                        code: "VERY_LARGE_TABLE".to_string(),
                        name: "Very Large Table".to_string(),
                        description: format!(
                            "Table '{}.{}' is very large ({} rows, {} bytes). Operations may take significant time.",
                            table.schema, table.table_name, table.row_count, table.size_bytes
                        ),
                        severity: RiskSeverity::High,
                        score_impact: -25.0,
                    });
                }
                SizeCategory::Large => {
                    factors.push(RiskFactor {
                        code: "LARGE_TABLE".to_string(),
                        name: "Large Table".to_string(),
                        description: format!(
                            "Table '{}.{}' is large ({} rows). Consider off-peak execution.",
                            table.schema, table.table_name, table.row_count
                        ),
                        severity: RiskSeverity::Medium,
                        score_impact: -15.0,
                    });
                }
                SizeCategory::Medium => {
                    factors.push(RiskFactor {
                        code: "MEDIUM_TABLE".to_string(),
                        name: "Medium Table".to_string(),
                        description: format!(
                            "Table '{}.{}' has {} rows.",
                            table.schema, table.table_name, table.row_count
                        ),
                        severity: RiskSeverity::Low,
                        score_impact: -5.0,
                    });
                }
                SizeCategory::Small => {
                    // No risk factor for small tables
                }
            }
        }
        
        factors
    }
    
    /// Calculate risk factors related to locking
    fn calculate_lock_risk(
        affected_tables: &[AffectedTableRisk],
        _proposal: &SchemaProposal,
    ) -> Vec<RiskFactor> {
        let mut factors = Vec::new();
        
        let tables_requiring_lock: Vec<_> = affected_tables.iter()
            .filter(|t| t.lock_required)
            .collect();
        
        if tables_requiring_lock.is_empty() {
            return factors;
        }
        
        let total_lock_ms: u64 = tables_requiring_lock.iter()
            .filter_map(|t| t.estimated_lock_ms)
            .sum();
        
        if total_lock_ms > 60_000 {
            factors.push(RiskFactor {
                code: "LONG_LOCK_DURATION".to_string(),
                name: "Long Lock Duration".to_string(),
                description: format!(
                    "Estimated lock duration is {} seconds. Production traffic may be significantly affected.",
                    total_lock_ms / 1000
                ),
                severity: RiskSeverity::Critical,
                score_impact: -40.0,
            });
        } else if total_lock_ms > 10_000 {
            factors.push(RiskFactor {
                code: "MODERATE_LOCK_DURATION".to_string(),
                name: "Moderate Lock Duration".to_string(),
                description: format!(
                    "Estimated lock duration is {} seconds. Some queries may timeout.",
                    total_lock_ms / 1000
                ),
                severity: RiskSeverity::High,
                score_impact: -25.0,
            });
        } else if total_lock_ms > 1_000 {
            factors.push(RiskFactor {
                code: "SHORT_LOCK_DURATION".to_string(),
                name: "Short Lock Duration".to_string(),
                description: format!(
                    "Estimated lock duration is {} ms. Brief impact expected.",
                    total_lock_ms
                ),
                severity: RiskSeverity::Medium,
                score_impact: -10.0,
            });
        }
        
        factors
    }
    
    /// Calculate risk factors for hot spots
    fn calculate_hotspot_risk(
        affected_tables: &[AffectedTableRisk],
        _semantic_map: &SemanticMap,
    ) -> Vec<RiskFactor> {
        let mut factors = Vec::new();
        
        for table in affected_tables {
            if table.is_hot_spot {
                factors.push(RiskFactor {
                    code: "HOT_SPOT_TABLE".to_string(),
                    name: "High Activity Table".to_string(),
                    description: format!(
                        "Table '{}.{}' has high read/write activity. Changes during peak hours may cause performance issues.",
                        table.schema, table.table_name
                    ),
                    severity: RiskSeverity::High,
                    score_impact: -20.0,
                });
            }
        }
        
        factors
    }
    
    /// Calculate risk factors for dependencies
    fn calculate_dependency_risk(impacts: &[DownstreamImpact]) -> Vec<RiskFactor> {
        let mut factors = Vec::new();
        
        let breaking_count = impacts.iter()
            .filter(|i| i.impact_type == ImpactType::Breaking)
            .count();
        
        let requires_update_count = impacts.iter()
            .filter(|i| i.impact_type == ImpactType::RequiresUpdate)
            .count();
        
        if breaking_count > 0 {
            factors.push(RiskFactor {
                code: "BREAKING_DEPENDENCIES".to_string(),
                name: "Breaking Dependencies".to_string(),
                description: format!(
                    "{} dependent object(s) will BREAK after this change. Manual fixes required.",
                    breaking_count
                ),
                severity: RiskSeverity::Critical,
                score_impact: -30.0 - (breaking_count as f64 * 5.0),
            });
        }
        
        if requires_update_count > 0 {
            factors.push(RiskFactor {
                code: "UPDATE_REQUIRED_DEPENDENCIES".to_string(),
                name: "Dependencies Need Update".to_string(),
                description: format!(
                    "{} dependent object(s) will need to be updated after this change.",
                    requires_update_count
                ),
                severity: RiskSeverity::Medium,
                score_impact: -10.0 - (requires_update_count as f64 * 2.0),
            });
        }
        
        factors
    }
    
    /// Calculate risk factors for destructive operations
    fn calculate_destructive_risk(proposal: &SchemaProposal) -> Vec<RiskFactor> {
        let mut factors = Vec::new();
        
        for change in &proposal.changes {
            match change {
                SchemaChange::DropTable(c) => {
                    factors.push(RiskFactor {
                        code: "DROP_TABLE".to_string(),
                        name: "Table Deletion".to_string(),
                        description: format!(
                            "Table '{}.{}' will be permanently deleted. DATA LOSS IS IRREVERSIBLE.",
                            c.schema, c.table_name
                        ),
                        severity: RiskSeverity::Critical,
                        score_impact: -35.0,
                    });
                }
                SchemaChange::DropColumn(c) => {
                    factors.push(RiskFactor {
                        code: "DROP_COLUMN".to_string(),
                        name: "Column Deletion".to_string(),
                        description: format!(
                            "Column '{}.{}.{}' will be permanently deleted. DATA LOSS IS IRREVERSIBLE.",
                            c.schema, c.table_name, c.column_name
                        ),
                        severity: RiskSeverity::High,
                        score_impact: -25.0,
                    });
                }
                _ => {}
            }
        }
        
        factors
    }
    
    /// Calculate risk factors for PII-related changes
    fn calculate_pii_risk(proposal: &SchemaProposal, semantic_map: &SemanticMap) -> Vec<RiskFactor> {
        let mut factors = Vec::new();
        
        // Check if any affected columns have PII classification
        for change in &proposal.changes {
            let pii_affected = match change {
                SchemaChange::DropColumn(c) => {
                    change_affects_pii(&c.schema, &c.table_name, &c.column_name, semantic_map)
                }
                SchemaChange::AlterColumn(c) => {
                    change_affects_pii(&c.schema, &c.table_name, &c.column_name, semantic_map)
                }
                _ => false,
            };
            
            if pii_affected {
                factors.push(RiskFactor {
                    code: "PII_COLUMN_CHANGE".to_string(),
                    name: "PII Column Affected".to_string(),
                    description: "Column has PII classification. Additional security review required.".to_string(),
                    severity: RiskSeverity::High,
                    score_impact: -20.0,
                });
            }
        }
        
        factors
    }
    
    /// Calculate overall safety score from risk factors
    fn calculate_safety_score(risk_factors: &[RiskFactor]) -> f64 {
        let mut score = 100.0;
        
        for factor in risk_factors {
            score += factor.score_impact;
        }
        
        // Clamp to 0-100 range
        score.max(0.0).min(100.0)
    }
    
    /// Estimate total lock duration in milliseconds
    fn estimate_lock_duration(
        affected_tables: &[AffectedTableRisk],
        _proposal: &SchemaProposal,
    ) -> Option<u64> {
        let total: u64 = affected_tables.iter()
            .filter_map(|t| t.estimated_lock_ms)
            .sum();
        
        if total > 0 {
            Some(total)
        } else {
            None
        }
    }
    
    /// Estimate lock duration for a single table operation
    fn estimate_single_table_lock(row_count: i64, size_bytes: i64, change: &SchemaChange) -> u64 {
        // Base estimation: ~0.1ms per 1000 rows for metadata operations
        // ~1ms per MB for data operations
        let base_ms = (row_count as f64 / 10000.0).max(1.0);
        let size_factor = (size_bytes as f64 / (1024.0 * 1024.0 * 100.0)).max(1.0);
        
        let operation_multiplier = match change {
            SchemaChange::AddColumn(c) if c.column.default_value.is_some() => 2.0,
            SchemaChange::AddColumn(_) => 0.5, // Fast for no default
            SchemaChange::AlterColumn(c) if c.new_type.is_some() => 5.0, // Type changes are slow
            SchemaChange::AlterColumn(c) if c.set_nullable == Some(false) => 3.0,
            SchemaChange::AlterColumn(_) => 1.0,
            SchemaChange::DropColumn(_) => 0.5,
            SchemaChange::AddForeignKey(_) => 2.0, // Needs to scan reference
            SchemaChange::AddPrimaryKey(_) => 3.0,
            SchemaChange::AddIndex(_) => 4.0, // Index builds can be slow
            _ => 1.0,
        };
        
        (base_ms * size_factor * operation_multiplier * 10.0) as u64
    }
    
    /// Generate recommendations based on analysis
    fn generate_recommendations(
        risk_factors: &[RiskFactor],
        affected_tables: &[AffectedTableRisk],
        downstream_impacts: &[DownstreamImpact],
        estimated_lock_ms: Option<u64>,
    ) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();
        
        // Check for very large tables
        let has_very_large = affected_tables.iter()
            .any(|t| matches!(t.size_category, SizeCategory::VeryLarge));
        
        if has_very_large {
            recommendations.push(Recommendation {
                priority: RecommendationPriority::Recommended,
                title: "Use pt-online-schema-change".to_string(),
                description: "For very large tables, consider using pt-online-schema-change or pg_repack to minimize lock time.".to_string(),
                action: Some("Install pt-online-schema-change or use CONCURRENTLY for indexes".to_string()),
            });
        }
        
        // Check for hot spots
        let has_hot_spots = affected_tables.iter().any(|t| t.is_hot_spot);
        if has_hot_spots {
            recommendations.push(Recommendation {
                priority: RecommendationPriority::Recommended,
                title: "Execute During Low Traffic".to_string(),
                description: "Affected tables have high activity. Execute during off-peak hours.".to_string(),
                action: Some("Schedule for maintenance window".to_string()),
            });
        }
        
        // Check for breaking dependencies
        let breaking = downstream_impacts.iter()
            .filter(|i| i.impact_type == ImpactType::Breaking)
            .collect::<Vec<_>>();
        
        if !breaking.is_empty() {
            recommendations.push(Recommendation {
                priority: RecommendationPriority::Required,
                title: "Fix Breaking Dependencies First".to_string(),
                description: format!(
                    "The following objects will break: {}. Update them before applying this change.",
                    breaking.iter().map(|b| format!("{}.{}", b.schema, b.name)).collect::<Vec<_>>().join(", ")
                ),
                action: Some("Update dependent objects before migration".to_string()),
            });
        }
        
        // Long lock warning
        if let Some(lock_ms) = estimated_lock_ms {
            if lock_ms > 30_000 {
                recommendations.push(Recommendation {
                    priority: RecommendationPriority::Required,
                    title: "Lock Duration Warning".to_string(),
                    description: format!(
                        "Estimated lock time is {} seconds. This may cause query timeouts and application errors.",
                        lock_ms / 1000
                    ),
                    action: Some("Consider breaking into smaller changes or using online schema change tools".to_string()),
                });
            }
        }
        
        // Always recommend backup for destructive operations
        let has_destructive = risk_factors.iter()
            .any(|f| f.code == "DROP_TABLE" || f.code == "DROP_COLUMN");
        
        if has_destructive {
            recommendations.push(Recommendation {
                priority: RecommendationPriority::Required,
                title: "Create Backup Before Execution".to_string(),
                description: "This change includes irreversible operations. Ensure you have a recent backup.".to_string(),
                action: Some("Run pg_dump or take a snapshot before proceeding".to_string()),
            });
        }
        
        // General best practice
        recommendations.push(Recommendation {
            priority: RecommendationPriority::Optional,
            title: "Test on Non-Production".to_string(),
            description: "Run this migration on a staging or development database first.".to_string(),
            action: None,
        });
        
        recommendations
    }
    
    // =============================================================================
    // HELPER METHODS
    // =============================================================================
    
    fn get_table_key_from_change(change: &SchemaChange) -> Option<String> {
        match change {
            SchemaChange::CreateTable(c) => Some(format!("{}.{}", c.schema, c.name)),
            SchemaChange::DropTable(c) => Some(format!("{}.{}", c.schema, c.table_name)),
            SchemaChange::RenameTable(c) => Some(format!("{}.{}", c.schema, c.old_name)),
            SchemaChange::AddColumn(c) => Some(format!("{}.{}", c.schema, c.table_name)),
            SchemaChange::DropColumn(c) => Some(format!("{}.{}", c.schema, c.table_name)),
            SchemaChange::AlterColumn(c) => Some(format!("{}.{}", c.schema, c.table_name)),
            SchemaChange::RenameColumn(c) => Some(format!("{}.{}", c.schema, c.table_name)),
            SchemaChange::AddForeignKey(c) => Some(format!("{}.{}", c.source_schema, c.source_table)),
            SchemaChange::DropForeignKey(c) => Some(format!("{}.{}", c.schema, c.table_name)),
            SchemaChange::AddPrimaryKey(c) => Some(format!("{}.{}", c.schema, c.table_name)),
            SchemaChange::DropPrimaryKey(c) => Some(format!("{}.{}", c.schema, c.table_name)),
            SchemaChange::AddUniqueConstraint(c) => Some(format!("{}.{}", c.schema, c.table_name)),
            SchemaChange::DropUniqueConstraint(c) => Some(format!("{}.{}", c.schema, c.table_name)),
            SchemaChange::AddIndex(c) => Some(format!("{}.{}", c.schema, c.table_name)),
            SchemaChange::DropIndex(c) => Some(format!("{}.unknown", c.schema)), // Index doesn't have table_name
            SchemaChange::SetPiiClassification(c) => Some(format!("{}.{}", c.schema, c.table_name)),
            SchemaChange::AddTag(c) => Self::parse_target_path(&c.target_path),
            SchemaChange::RemoveTag(c) => Self::parse_target_path(&c.target_path),
            SchemaChange::SetDescription(c) => Self::parse_target_path(&c.target_path),
        }
    }
    
    fn parse_target_path(path: &str) -> Option<String> {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.len() >= 2 {
            Some(format!("{}.{}", parts[0], parts[1]))
        } else {
            None
        }
    }
    
    fn split_table_key(key: &str) -> (String, String) {
        let parts: Vec<&str> = key.split('.').collect();
        if parts.len() == 2 {
            (parts[0].to_string(), parts[1].to_string())
        } else {
            ("public".to_string(), key.to_string())
        }
    }
    
    fn change_requires_lock(change: &SchemaChange) -> bool {
        matches!(
            change,
            SchemaChange::AddColumn(_)
                | SchemaChange::DropColumn(_)
                | SchemaChange::AlterColumn(_)
                | SchemaChange::AddForeignKey(_)
                | SchemaChange::DropForeignKey(_)
                | SchemaChange::AddPrimaryKey(_)
                | SchemaChange::DropPrimaryKey(_)
                | SchemaChange::AddUniqueConstraint(_)
                | SchemaChange::DropUniqueConstraint(_)
        )
    }
}

/// Check if a column has PII classification
fn change_affects_pii(schema: &str, table: &str, column: &str, semantic_map: &SemanticMap) -> bool {
    // Check if the column has PII classification in the schema
    let table_key = format!("{}.{}", schema, table);
    if let Some(table) = semantic_map.schema.tables.iter()
        .find(|t| format!("{}.{}", t.schema, t.name) == table_key)
    {
        if let Some(col) = table.columns.iter().find(|c| c.name == column) {
            return col.pii_classification.is_some() 
                && col.pii_classification != Some(crate::introspection::PiiLevel::None);
        }
    }
    false
}
