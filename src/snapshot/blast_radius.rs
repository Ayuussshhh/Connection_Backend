//! Blast Radius Analyzer
//!
//! "What breaks if I change this column?"
//! This module walks the dependency graph to find all downstream impacts.

#[allow(unused_imports)]
use crate::introspection::{ForeignKey, SchemaSnapshot, Table};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// An object impacted by a change
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactedObject {
    /// Type of object
    pub object_type: ImpactType,
    /// Full path (schema.table or schema.table.column)
    pub path: String,
    /// Relationship type
    pub relationship: RelationshipType,
    /// Distance from the source (hops count)
    pub distance: u32,
    /// Human-readable impact description
    pub impact: String,
    /// Is this a direct or transitive dependency
    pub is_direct: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImpactType {
    Table,
    Column,
    View,
    Index,
    Trigger,
    Function,
    Query, // Future: tracked queries
    Service, // Future: tracked services
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipType {
    /// Foreign key points TO this table/column
    ForeignKeyTo,
    /// Foreign key points FROM this table/column
    ForeignKeyFrom,
    /// View depends on this table/column
    ViewDependency,
    /// Index on this column
    IndexOn,
    /// Query reads from this
    QueryRead,
    /// Query writes to this
    QueryWrite,
}

/// Complete blast radius analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlastRadius {
    /// The source of the change
    pub source_path: String,
    /// All impacted objects
    pub impacted: Vec<ImpactedObject>,
    /// Summary counts
    pub summary: BlastRadiusSummary,
    /// Risk assessment
    pub risk_level: BlastRiskLevel,
    /// Human-readable explanation
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlastRadiusSummary {
    pub direct_tables: usize,
    pub transitive_tables: usize,
    pub total_tables: usize,
    pub total_columns: usize,
    pub total_indexes: usize,
    pub max_depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlastRiskLevel {
    None,      // No dependencies
    Contained, // Only direct dependencies
    Spreading, // Has transitive dependencies
    Pandemic,  // Impacts most of the schema
}

/// The blast radius analyzer
pub struct BlastRadiusAnalyzer;

impl BlastRadiusAnalyzer {
    /// Analyze blast radius for a table change
    pub fn analyze_table(snapshot: &SchemaSnapshot, schema: &str, table_name: &str) -> BlastRadius {
        let source_path = format!("{}.{}", schema, table_name);
        let mut impacted = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        
        // Build dependency graph
        let deps = Self::build_dependency_graph(snapshot);
        
        // BFS to find all downstream dependencies
        let mut queue: VecDeque<(String, u32, bool)> = VecDeque::new();
        
        // Add direct dependencies
        if let Some(direct_deps) = deps.get(&source_path) {
            for dep in direct_deps {
                queue.push_back((dep.clone(), 1, true));
            }
        }
        
        while let Some((path, distance, _is_direct)) = queue.pop_front() {
            if visited.contains(&path) {
                continue;
            }
            visited.insert(path.clone());
            
            // Determine relationship type
            let relationship = Self::determine_relationship(snapshot, &source_path, &path);
            
            impacted.push(ImpactedObject {
                object_type: ImpactType::Table,
                path: path.clone(),
                relationship,
                distance,
                impact: Self::describe_impact(&relationship, &source_path, &path),
                is_direct: distance == 1,
            });
            
            // Add transitive dependencies
            if let Some(transitive_deps) = deps.get(&path) {
                for dep in transitive_deps {
                    if !visited.contains(dep) {
                        queue.push_back((dep.clone(), distance + 1, false));
                    }
                }
            }
        }
        
        let summary = Self::calculate_summary(&impacted);
        let risk_level = Self::assess_risk(&summary, snapshot.tables.len());
        let explanation = Self::generate_explanation(&source_path, &summary, &risk_level);
        
        BlastRadius {
            source_path,
            impacted,
            summary,
            risk_level,
            explanation,
        }
    }

    /// Analyze blast radius for a specific column
    pub fn analyze_column(
        snapshot: &SchemaSnapshot,
        schema: &str,
        table_name: &str,
        column_name: &str,
    ) -> BlastRadius {
        let source_path = format!("{}.{}.{}", schema, table_name, column_name);
        let table_path = format!("{}.{}", schema, table_name);
        let mut impacted = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        
        // Find FKs that reference this column
        for fk in &snapshot.foreign_keys {
            let fk_source = format!("{}.{}", fk.source_schema, fk.source_table);
            let fk_target = format!("{}.{}", fk.referenced_schema, fk.referenced_table);
            
            // Check if this FK involves our column
            let involves_column = 
                (fk_target == table_path && fk.referenced_columns.contains(&column_name.to_string())) ||
                (fk_source == table_path && fk.source_columns.contains(&column_name.to_string()));
            
            if involves_column {
                let other_table = if fk_target == table_path { &fk_source } else { &fk_target };
                
                if !visited.contains(other_table) {
                    visited.insert(other_table.clone());
                    
                    let relationship = if fk_target == table_path {
                        RelationshipType::ForeignKeyTo
                    } else {
                        RelationshipType::ForeignKeyFrom
                    };
                    
                    impacted.push(ImpactedObject {
                        object_type: ImpactType::Table,
                        path: other_table.clone(),
                        relationship,
                        distance: 1,
                        impact: format!(
                            "Table {} has FK relationship via {}",
                            other_table.split('.').last().unwrap_or(""),
                            fk.constraint_name
                        ),
                        is_direct: true,
                    });
                }
            }
        }
        
        // Find indexes on this column
        for idx in &snapshot.indexes {
            if idx.schema == schema && idx.table == table_name && idx.columns.contains(&column_name.to_string()) {
                impacted.push(ImpactedObject {
                    object_type: ImpactType::Index,
                    path: format!("{}.{}", idx.schema, idx.name),
                    relationship: RelationshipType::IndexOn,
                    distance: 1,
                    impact: format!(
                        "Index {} includes this column ({})",
                        idx.name,
                        if idx.is_unique { "UNIQUE" } else { "non-unique" }
                    ),
                    is_direct: true,
                });
            }
        }
        
        let summary = Self::calculate_summary(&impacted);
        let risk_level = Self::assess_risk(&summary, snapshot.tables.len());
        let explanation = Self::generate_explanation(&source_path, &summary, &risk_level);
        
        BlastRadius {
            source_path,
            impacted,
            summary,
            risk_level,
            explanation,
        }
    }

    /// Build a dependency graph from foreign keys
    fn build_dependency_graph(snapshot: &SchemaSnapshot) -> HashMap<String, Vec<String>> {
        let mut deps: HashMap<String, Vec<String>> = HashMap::new();
        
        for fk in &snapshot.foreign_keys {
            let source = format!("{}.{}", fk.source_schema, fk.source_table);
            let target = format!("{}.{}", fk.referenced_schema, fk.referenced_table);
            
            // Target table has dependents
            deps.entry(target.clone())
                .or_default()
                .push(source.clone());
        }
        
        deps
    }

    fn determine_relationship(
        snapshot: &SchemaSnapshot,
        source: &str,
        target: &str,
    ) -> RelationshipType {
        for fk in &snapshot.foreign_keys {
            let fk_target = format!("{}.{}", fk.referenced_schema, fk.referenced_table);
            let fk_source = format!("{}.{}", fk.source_schema, fk.source_table);
            
            if fk_target == *source && fk_source == *target {
                return RelationshipType::ForeignKeyTo;
            }
            if fk_source == *source && fk_target == *target {
                return RelationshipType::ForeignKeyFrom;
            }
        }
        RelationshipType::ForeignKeyTo // Default
    }

    fn describe_impact(relationship: &RelationshipType, source: &str, target: &str) -> String {
        let target_name = target.split('.').last().unwrap_or(target);
        let source_name = source.split('.').last().unwrap_or(source);
        
        match relationship {
            RelationshipType::ForeignKeyTo => {
                format!("Table {} references {} via foreign key", target_name, source_name)
            }
            RelationshipType::ForeignKeyFrom => {
                format!("Table {} is referenced by {} via foreign key", target_name, source_name)
            }
            RelationshipType::ViewDependency => {
                format!("View {} depends on {}", target_name, source_name)
            }
            RelationshipType::IndexOn => {
                format!("Index on {} includes column from {}", target_name, source_name)
            }
            RelationshipType::QueryRead => {
                format!("Query reads from {}", target_name)
            }
            RelationshipType::QueryWrite => {
                format!("Query writes to {}", target_name)
            }
        }
    }

    fn calculate_summary(impacted: &[ImpactedObject]) -> BlastRadiusSummary {
        let direct_tables = impacted.iter()
            .filter(|i| i.is_direct && i.object_type == ImpactType::Table)
            .count();
        let transitive_tables = impacted.iter()
            .filter(|i| !i.is_direct && i.object_type == ImpactType::Table)
            .count();
        let total_tables = direct_tables + transitive_tables;
        let total_columns = impacted.iter()
            .filter(|i| i.object_type == ImpactType::Column)
            .count();
        let total_indexes = impacted.iter()
            .filter(|i| i.object_type == ImpactType::Index)
            .count();
        let max_depth = impacted.iter()
            .map(|i| i.distance)
            .max()
            .unwrap_or(0);
        
        BlastRadiusSummary {
            direct_tables,
            transitive_tables,
            total_tables,
            total_columns,
            total_indexes,
            max_depth,
        }
    }

    fn assess_risk(summary: &BlastRadiusSummary, total_tables: usize) -> BlastRiskLevel {
        if summary.total_tables == 0 {
            return BlastRiskLevel::None;
        }
        
        let impact_ratio = summary.total_tables as f64 / total_tables as f64;
        
        if impact_ratio > 0.5 {
            BlastRiskLevel::Pandemic
        } else if summary.transitive_tables > 0 {
            BlastRiskLevel::Spreading
        } else {
            BlastRiskLevel::Contained
        }
    }

    fn generate_explanation(
        source: &str,
        summary: &BlastRadiusSummary,
        risk: &BlastRiskLevel,
    ) -> String {
        let source_name = source.split('.').last().unwrap_or(source);
        
        match risk {
            BlastRiskLevel::None => {
                format!("No dependencies found for {}. Safe to modify.", source_name)
            }
            BlastRiskLevel::Contained => {
                format!(
                    "Changes to {} will directly affect {} table(s). No transitive dependencies.",
                    source_name, summary.direct_tables
                )
            }
            BlastRiskLevel::Spreading => {
                format!(
                    "⚠️ Changes to {} will cascade to {} tables ({} direct, {} transitive). Review carefully.",
                    source_name, summary.total_tables, summary.direct_tables, summary.transitive_tables
                )
            }
            BlastRiskLevel::Pandemic => {
                format!(
                    "🚨 CRITICAL: Changes to {} will impact {} tables! This is a core entity.",
                    source_name, summary.total_tables
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::introspection::{Column, Table, Index, ForeignKey};
    use uuid::Uuid;
    use chrono::Utc;

    fn create_test_snapshot() -> SchemaSnapshot {
        SchemaSnapshot {
            id: Uuid::new_v4(),
            connection_id: Uuid::new_v4(),
            version: 1,
            captured_at: Utc::now(),
            tables: vec![
                Table {
                    name: "users".to_string(),
                    schema: "public".to_string(),
                    columns: vec![
                        Column {
                            name: "id".to_string(),
                            data_type: "integer".to_string(),
                            nullable: false,
                            default_value: None,
                            is_primary_key: true,
                            is_unique: true,
                            ordinal_position: 1,
                            pii_classification: None,
                            description: None,
                            tags: vec![],
                        }
                    ],
                    primary_key: None,
                    position: None,
                    color: None,
                    collapsed: false,
                    governance: Default::default(),
                },
                Table {
                    name: "orders".to_string(),
                    schema: "public".to_string(),
                    columns: vec![],
                    primary_key: None,
                    position: None,
                    color: None,
                    collapsed: false,
                    governance: Default::default(),
                },
            ],
            foreign_keys: vec![
                ForeignKey {
                    constraint_name: "orders_user_fk".to_string(),
                    source_schema: "public".to_string(),
                    source_table: "orders".to_string(),
                    source_columns: vec!["user_id".to_string()],
                    referenced_schema: "public".to_string(),
                    referenced_table: "users".to_string(),
                    referenced_columns: vec!["id".to_string()],
                    on_update: "NO ACTION".to_string(),
                    on_delete: "CASCADE".to_string(),
                }
            ],
            indexes: vec![],
            checksum: "test".to_string(),
        }
    }

    #[test]
    fn test_analyze_table_finds_dependents() {
        let snapshot = create_test_snapshot();
        let result = BlastRadiusAnalyzer::analyze_table(&snapshot, "public", "users");
        
        assert_eq!(result.impacted.len(), 1);
        assert_eq!(result.impacted[0].path, "public.orders");
        assert_eq!(result.summary.direct_tables, 1);
    }
}
