//! Rules Engine
//!
//! "Junior-proof" guardrails for database changes.
//! This is what managers pay for - automated enforcement.

use crate::introspection::SchemaSnapshot;
use crate::snapshot::diff::{ChangeType, ObjectType, SchemaDiff, SchemaDiffItem};
#[allow(unused_imports)]
use crate::snapshot::blast_radius::{BlastRadius, BlastRadiusAnalyzer};
use serde::{Deserialize, Serialize};

/// Rule severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,   // Blocks approval but not execution
    Block,   // Blocks execution entirely
}

/// A rule violation found during analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleViolation {
    pub rule_id: String,
    pub rule_name: String,
    pub severity: Severity,
    pub message: String,
    pub affected_object: String,
    pub suggestion: Option<String>,
}

/// A governance rule definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub severity: Severity,
    pub enabled: bool,
    /// Category for grouping
    pub category: RuleCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleCategory {
    DataLoss,
    Performance,
    Security,
    Compatibility,
    BestPractice,
}

/// Result of rules evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesResult {
    pub violations: Vec<RuleViolation>,
    pub has_blockers: bool,
    pub has_errors: bool,
    pub has_warnings: bool,
    pub summary: RulesSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesSummary {
    pub total_rules_checked: usize,
    pub violations_by_severity: std::collections::HashMap<String, usize>,
    pub can_proceed: bool,
    pub requires_approval: bool,
}

/// The rules engine that enforces governance policies
pub struct RulesEngine {
    rules: Vec<Rule>,
}

impl RulesEngine {
    /// Create a new rules engine with default rules
    pub fn new() -> Self {
        Self {
            rules: Self::default_rules(),
        }
    }

    /// Get all configured rules
    pub fn list_rules(&self) -> &[Rule] {
        &self.rules
    }

    /// Evaluate a schema diff against all rules
    pub fn evaluate(&self, diff: &SchemaDiff, snapshot: &SchemaSnapshot) -> RulesResult {
        let mut violations = Vec::new();
        
        for change in &diff.changes {
            // Run each rule against each change
            violations.extend(self.check_drop_column_rule(change, snapshot));
            violations.extend(self.check_drop_table_rule(change, snapshot));
            violations.extend(self.check_index_removal_rule(change, snapshot));
            violations.extend(self.check_type_change_rule(change));
            violations.extend(self.check_not_null_without_default(change));
            violations.extend(self.check_rename_without_alias(change));
            violations.extend(self.check_pk_modification(change));
            violations.extend(self.check_cascade_delete(change, snapshot));
        }
        
        let has_blockers = violations.iter().any(|v| v.severity == Severity::Block);
        let has_errors = violations.iter().any(|v| v.severity == Severity::Error);
        let has_warnings = violations.iter().any(|v| v.severity == Severity::Warning);
        
        let mut violations_by_severity = std::collections::HashMap::new();
        for v in &violations {
            let key = format!("{:?}", v.severity).to_lowercase();
            *violations_by_severity.entry(key).or_insert(0) += 1;
        }
        
        let can_proceed = !has_blockers;
        let requires_approval = has_errors || has_warnings;
        
        RulesResult {
            violations,
            has_blockers,
            has_errors,
            has_warnings,
            summary: RulesSummary {
                total_rules_checked: self.rules.len(),
                violations_by_severity,
                can_proceed,
                requires_approval,
            },
        }
    }

    /// Rule: Block dropping a column with dependencies
    fn check_drop_column_rule(
        &self,
        change: &SchemaDiffItem,
        snapshot: &SchemaSnapshot,
    ) -> Vec<RuleViolation> {
        let mut violations = Vec::new();
        
        if change.object_type != ObjectType::Column || change.change_type != ChangeType::Removed {
            return violations;
        }
        
        // Parse path: schema.table.column
        let parts: Vec<&str> = change.object_path.split('.').collect();
        if parts.len() != 3 {
            return violations;
        }
        
        let schema = parts[0];
        let table = parts[1];
        let column = parts[2];
        
        // Check blast radius
        let blast = BlastRadiusAnalyzer::analyze_column(snapshot, schema, table, column);
        
        if blast.impacted.len() > 0 {
            violations.push(RuleViolation {
                rule_id: "R001".to_string(),
                rule_name: "Column Drop with Dependencies".to_string(),
                severity: Severity::Block,
                message: format!(
                    "Cannot drop column {} - it has {} dependent objects",
                    change.object_path,
                    blast.impacted.len()
                ),
                affected_object: change.object_path.clone(),
                suggestion: Some(format!(
                    "First remove or update these dependencies: {}",
                    blast.impacted.iter()
                        .take(3)
                        .map(|i| i.path.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            });
        }
        
        violations
    }

    /// Rule: Block dropping tables with foreign key dependencies
    fn check_drop_table_rule(
        &self,
        change: &SchemaDiffItem,
        snapshot: &SchemaSnapshot,
    ) -> Vec<RuleViolation> {
        let mut violations = Vec::new();
        
        if change.object_type != ObjectType::Table || change.change_type != ChangeType::Removed {
            return violations;
        }
        
        let parts: Vec<&str> = change.object_path.split('.').collect();
        if parts.len() != 2 {
            return violations;
        }
        
        let schema = parts[0];
        let table = parts[1];
        
        let blast = BlastRadiusAnalyzer::analyze_table(snapshot, schema, table);
        
        if blast.summary.total_tables > 0 {
            violations.push(RuleViolation {
                rule_id: "R002".to_string(),
                rule_name: "Table Drop with Dependencies".to_string(),
                severity: Severity::Block,
                message: format!(
                    "Cannot drop table {} - {} other tables depend on it",
                    change.object_path,
                    blast.summary.total_tables
                ),
                affected_object: change.object_path.clone(),
                suggestion: Some("Drop dependent tables first, or update their foreign keys".to_string()),
            });
        }
        
        violations
    }

    /// Rule: Warn on index removal from large tables
    fn check_index_removal_rule(
        &self,
        change: &SchemaDiffItem,
        _snapshot: &SchemaSnapshot,
    ) -> Vec<RuleViolation> {
        let mut violations = Vec::new();
        
        if change.object_type != ObjectType::Index || change.change_type != ChangeType::Removed {
            return violations;
        }
        
        // Check if it's a unique index
        if let Some(before) = &change.before {
            if before.get("isUnique").and_then(|v| v.as_bool()).unwrap_or(false) {
                violations.push(RuleViolation {
                    rule_id: "R003".to_string(),
                    rule_name: "Unique Index Removal".to_string(),
                    severity: Severity::Block,
                    message: format!(
                        "Removing unique index {} could allow duplicate values",
                        change.object_path
                    ),
                    affected_object: change.object_path.clone(),
                    suggestion: Some("Consider adding a unique constraint if uniqueness is required".to_string()),
                });
            } else {
                violations.push(RuleViolation {
                    rule_id: "R004".to_string(),
                    rule_name: "Index Removal Performance Impact".to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "Removing index {} may impact query performance",
                        change.object_path
                    ),
                    affected_object: change.object_path.clone(),
                    suggestion: Some("Review query plans before removing indexes".to_string()),
                });
            }
        }
        
        violations
    }

    /// Rule: Warn on breaking type changes
    fn check_type_change_rule(&self, change: &SchemaDiffItem) -> Vec<RuleViolation> {
        let mut violations = Vec::new();
        
        if change.object_type != ObjectType::Column || change.change_type != ChangeType::Modified {
            return violations;
        }
        
        // Check if this is a type change
        let before_type = change.before.as_ref()
            .and_then(|b| b.get("dataType"))
            .and_then(|v| v.as_str());
        let after_type = change.after.as_ref()
            .and_then(|a| a.get("dataType"))
            .and_then(|v| v.as_str());
        
        if let (Some(before), Some(after)) = (before_type, after_type) {
            if before != after {
                let is_narrowing = Self::is_narrowing_conversion(before, after);
                
                if is_narrowing {
                    violations.push(RuleViolation {
                        rule_id: "R005".to_string(),
                        rule_name: "Narrowing Type Conversion".to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "Type change {} → {} may cause data loss or truncation in {}",
                            before, after, change.object_path
                        ),
                        affected_object: change.object_path.clone(),
                        suggestion: Some(format!(
                            "Consider: 1) Add new column with {}, 2) Migrate data, 3) Drop old column",
                            after
                        )),
                    });
                }
            }
        }
        
        violations
    }

    /// Rule: Block NOT NULL without default on existing columns
    fn check_not_null_without_default(&self, change: &SchemaDiffItem) -> Vec<RuleViolation> {
        let mut violations = Vec::new();
        
        if change.object_type != ObjectType::Column {
            return violations;
        }
        
        // Check if nullable changed from true to false
        let before_nullable = change.before.as_ref()
            .and_then(|b| b.get("nullable"))
            .and_then(|v| v.as_bool());
        let after_nullable = change.after.as_ref()
            .and_then(|a| a.get("nullable"))
            .and_then(|v| v.as_bool());
        let after_default = change.after.as_ref()
            .and_then(|a| a.get("defaultValue"));
        
        if before_nullable == Some(true) && after_nullable == Some(false) && after_default.is_none() {
            violations.push(RuleViolation {
                rule_id: "R006".to_string(),
                rule_name: "NOT NULL Without Default".to_string(),
                severity: Severity::Block,
                message: format!(
                    "Cannot set {} to NOT NULL without a default value - existing NULLs will fail",
                    change.object_path
                ),
                affected_object: change.object_path.clone(),
                suggestion: Some("Either: 1) Set a default value, 2) Backfill NULLs first, 3) Make it nullable".to_string()),
            });
        }
        
        violations
    }

    /// Rule: Warn on renames without backward compatibility
    fn check_rename_without_alias(&self, change: &SchemaDiffItem) -> Vec<RuleViolation> {
        let mut violations = Vec::new();
        
        if change.change_type != ChangeType::Renamed {
            return violations;
        }
        
        violations.push(RuleViolation {
            rule_id: "R007".to_string(),
            rule_name: "Rename Without Alias".to_string(),
            severity: Severity::Warning,
            message: format!(
                "Renaming {} may break existing queries and applications",
                change.object_path
            ),
            affected_object: change.object_path.clone(),
            suggestion: Some("Consider creating a view alias for backward compatibility".to_string()),
        });
        
        violations
    }

    /// Rule: Block primary key modifications
    fn check_pk_modification(&self, change: &SchemaDiffItem) -> Vec<RuleViolation> {
        let mut violations = Vec::new();
        
        if change.object_type != ObjectType::Column || change.change_type != ChangeType::Modified {
            return violations;
        }
        
        let before_pk = change.before.as_ref()
            .and_then(|b| b.get("isPrimaryKey"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let after_pk = change.after.as_ref()
            .and_then(|a| a.get("isPrimaryKey"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        
        if before_pk && !after_pk {
            violations.push(RuleViolation {
                rule_id: "R008".to_string(),
                rule_name: "Primary Key Removal".to_string(),
                severity: Severity::Block,
                message: format!(
                    "Removing {} from primary key requires careful migration",
                    change.object_path
                ),
                affected_object: change.object_path.clone(),
                suggestion: Some("Create a new table with correct PK and migrate data".to_string()),
            });
        }
        
        violations
    }

    /// Rule: Warn on CASCADE DELETE additions
    fn check_cascade_delete(
        &self,
        change: &SchemaDiffItem,
        _snapshot: &SchemaSnapshot,
    ) -> Vec<RuleViolation> {
        let mut violations = Vec::new();
        
        if change.object_type != ObjectType::ForeignKey || change.change_type != ChangeType::Added {
            return violations;
        }
        
        let on_delete = change.after.as_ref()
            .and_then(|a| a.get("onDelete"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        
        if on_delete.to_uppercase() == "CASCADE" {
            violations.push(RuleViolation {
                rule_id: "R009".to_string(),
                rule_name: "CASCADE DELETE Addition".to_string(),
                severity: Severity::Warning,
                message: format!(
                    "Adding CASCADE DELETE on {} may cause unexpected data loss",
                    change.object_path
                ),
                affected_object: change.object_path.clone(),
                suggestion: Some("Use RESTRICT or SET NULL if data preservation is important".to_string()),
            });
        }
        
        violations
    }

    fn is_narrowing_conversion(from: &str, to: &str) -> bool {
        let from_lower = from.to_lowercase();
        let to_lower = to.to_lowercase();
        
        // Narrowing conversions (data loss risk)
        let narrowing_pairs = [
            ("bigint", "integer"),
            ("bigint", "smallint"),
            ("integer", "smallint"),
            ("double precision", "real"),
            ("text", "varchar"),
            ("varchar", "char"),
            ("timestamp", "date"),
        ];
        
        for (wide, narrow) in narrowing_pairs {
            if from_lower.contains(wide) && to_lower.contains(narrow) {
                return true;
            }
        }
        
        false
    }

    fn default_rules() -> Vec<Rule> {
        vec![
            Rule {
                id: "R001".to_string(),
                name: "Column Drop with Dependencies".to_string(),
                description: "Block dropping columns that have foreign key references".to_string(),
                severity: Severity::Block,
                enabled: true,
                category: RuleCategory::DataLoss,
            },
            Rule {
                id: "R002".to_string(),
                name: "Table Drop with Dependencies".to_string(),
                description: "Block dropping tables that are referenced by other tables".to_string(),
                severity: Severity::Block,
                enabled: true,
                category: RuleCategory::DataLoss,
            },
            Rule {
                id: "R003".to_string(),
                name: "Unique Index Removal".to_string(),
                description: "Block removing unique indexes that enforce data integrity".to_string(),
                severity: Severity::Block,
                enabled: true,
                category: RuleCategory::DataLoss,
            },
            Rule {
                id: "R004".to_string(),
                name: "Index Removal Performance".to_string(),
                description: "Warn when removing indexes that may impact performance".to_string(),
                severity: Severity::Warning,
                enabled: true,
                category: RuleCategory::Performance,
            },
            Rule {
                id: "R005".to_string(),
                name: "Narrowing Type Conversion".to_string(),
                description: "Error on type changes that may cause data truncation".to_string(),
                severity: Severity::Error,
                enabled: true,
                category: RuleCategory::DataLoss,
            },
            Rule {
                id: "R006".to_string(),
                name: "NOT NULL Without Default".to_string(),
                description: "Block setting NOT NULL on columns without default values".to_string(),
                severity: Severity::Block,
                enabled: true,
                category: RuleCategory::Compatibility,
            },
            Rule {
                id: "R007".to_string(),
                name: "Rename Without Alias".to_string(),
                description: "Warn when renaming objects without backward compatibility".to_string(),
                severity: Severity::Warning,
                enabled: true,
                category: RuleCategory::Compatibility,
            },
            Rule {
                id: "R008".to_string(),
                name: "Primary Key Removal".to_string(),
                description: "Block removing columns from primary keys".to_string(),
                severity: Severity::Block,
                enabled: true,
                category: RuleCategory::DataLoss,
            },
            Rule {
                id: "R009".to_string(),
                name: "CASCADE DELETE Addition".to_string(),
                description: "Warn when adding CASCADE DELETE foreign keys".to_string(),
                severity: Severity::Warning,
                enabled: true,
                category: RuleCategory::DataLoss,
            },
        ]
    }
}

impl Default for RulesEngine {
    fn default() -> Self {
        Self::new()
    }
}
