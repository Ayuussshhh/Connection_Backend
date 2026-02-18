//! Risk analyzer for schema changes
//!
//! Analyzes proposed changes to estimate risk levels and impacts.

use crate::error::AppError;
use crate::proposal::*;
use deadpool_postgres::Pool;

pub struct RiskAnalyzer;

impl RiskAnalyzer {
    /// Analyze a set of changes and produce a risk assessment
    pub async fn analyze(
        pool: &Pool,
        changes: &[SchemaChange],
    ) -> Result<RiskAnalysis, AppError> {
        let client = pool.get().await?;
        
        let mut risk_factors = Vec::new();
        let mut locked_tables = Vec::new();
        let mut downstream_impacts = Vec::new();
        let mut estimated_duration = 0.0f64;
        
        for change in changes {
            // Analyze each change
            let (factors, duration) = Self::analyze_change(&client, change).await?;
            risk_factors.extend(factors);
            estimated_duration += duration;
            
            // Check for table locks
            if change.requires_table_lock() {
                if let Some((schema, table)) = change.target_table() {
                    locked_tables.push(format!("{}.{}", schema, table));
                }
            }
            
            // Check for downstream impacts
            if let Some((schema, table)) = change.target_table() {
                let impacts = Self::find_downstream_impacts(&client, &schema, &table).await?;
                downstream_impacts.extend(impacts);
            }
        }
        
        // Calculate risk score
        let risk_score = Self::calculate_risk_score(&risk_factors, &locked_tables, changes);
        let risk_level = Self::score_to_level(risk_score);
        
        // Calculate potential downtime
        let potential_downtime = if locked_tables.is_empty() {
            0.0
        } else {
            estimated_duration * 1.5 // Add safety margin
        };
        
        // Generate recommendations
        let recommendations = Self::generate_recommendations(&risk_factors, &locked_tables, changes);
        
        Ok(RiskAnalysis {
            risk_score,
            risk_level,
            estimated_duration_seconds: estimated_duration,
            locked_tables,
            potential_downtime_seconds: potential_downtime,
            downstream_impacts,
            risk_factors,
            recommendations,
        })
    }

    async fn analyze_change(
        client: &deadpool_postgres::Client,
        change: &SchemaChange,
    ) -> Result<(Vec<RiskFactor>, f64), AppError> {
        let mut factors = Vec::new();
        let mut duration = 0.1; // Base duration
        
        match change {
            SchemaChange::CreateTable(_) => {
                duration = 0.5;
            }
            SchemaChange::DropTable(c) => {
                factors.push(RiskFactor {
                    category: "Data Loss".to_string(),
                    description: format!("Dropping table {}.{} will permanently delete all data", c.schema, c.table_name),
                    severity: RiskLevel::Critical,
                    mitigation: Some("Ensure data is backed up before proceeding".to_string()),
                });
                
                // Check table size
                if let Ok(row_count) = Self::get_table_row_count(client, &c.schema, &c.table_name).await {
                    if row_count > 0 {
                        factors.push(RiskFactor {
                            category: "Data Loss".to_string(),
                            description: format!("Table contains {} rows that will be deleted", row_count),
                            severity: RiskLevel::High,
                            mitigation: None,
                        });
                    }
                }
                duration = 1.0;
            }
            SchemaChange::AddColumn(c) => {
                if !c.column.nullable && c.column.default_value.is_none() {
                    factors.push(RiskFactor {
                        category: "Schema Lock".to_string(),
                        description: "Adding NOT NULL column without default requires table rewrite".to_string(),
                        severity: RiskLevel::High,
                        mitigation: Some("Add with default value or make nullable first".to_string()),
                    });
                    duration = 10.0; // Table rewrite is slow
                } else {
                    duration = 0.5;
                }
            }
            SchemaChange::DropColumn(c) => {
                factors.push(RiskFactor {
                    category: "Data Loss".to_string(),
                    description: format!("Dropping column {} will delete all its data", c.column_name),
                    severity: RiskLevel::High,
                    mitigation: None,
                });
                duration = 0.5;
            }
            SchemaChange::ModifyColumn(c) => {
                if c.new_type.is_some() {
                    factors.push(RiskFactor {
                        category: "Schema Lock".to_string(),
                        description: "Type change requires table rewrite".to_string(),
                        severity: RiskLevel::Medium,
                        mitigation: None,
                    });
                    duration = 15.0;
                }
                if c.new_nullable == Some(false) {
                    factors.push(RiskFactor {
                        category: "Constraint".to_string(),
                        description: "Setting NOT NULL requires validating all existing rows".to_string(),
                        severity: RiskLevel::Medium,
                        mitigation: None,
                    });
                }
            }
            SchemaChange::AddForeignKey(_) => {
                factors.push(RiskFactor {
                    category: "Constraint".to_string(),
                    description: "Adding foreign key requires validating all existing rows".to_string(),
                    severity: RiskLevel::Low,
                    mitigation: None,
                });
                duration = 5.0;
            }
            SchemaChange::AddIndex(c) => {
                if c.concurrent {
                    factors.push(RiskFactor {
                        category: "Best Practice".to_string(),
                        description: "Using CONCURRENTLY to avoid blocking writes".to_string(),
                        severity: RiskLevel::Low,
                        mitigation: None,
                    });
                    duration = 30.0; // Concurrent is slower but doesn't block
                } else {
                    factors.push(RiskFactor {
                        category: "Table Lock".to_string(),
                        description: "Index creation will lock the table for writes".to_string(),
                        severity: RiskLevel::Medium,
                        mitigation: Some("Consider using CONCURRENTLY option".to_string()),
                    });
                    duration = 10.0;
                }
            }
            _ => {}
        }
        
        Ok((factors, duration))
    }

    async fn get_table_row_count(
        client: &deadpool_postgres::Client,
        schema: &str,
        table: &str,
    ) -> Result<i64, AppError> {
        let query = format!(
            "SELECT reltuples::bigint AS estimate FROM pg_class c 
             JOIN pg_namespace n ON n.oid = c.relnamespace 
             WHERE n.nspname = $1 AND c.relname = $2"
        );
        
        let row = client.query_one(&query, &[&schema, &table]).await?;
        Ok(row.get::<_, i64>("estimate"))
    }

    async fn find_downstream_impacts(
        client: &deadpool_postgres::Client,
        schema: &str,
        table: &str,
    ) -> Result<Vec<DownstreamImpact>, AppError> {
        let mut impacts = Vec::new();
        
        // Find foreign keys referencing this table
        let query = r#"
            SELECT
                tc.table_schema,
                tc.table_name,
                tc.constraint_name
            FROM information_schema.table_constraints tc
            JOIN information_schema.constraint_column_usage ccu 
                ON tc.constraint_name = ccu.constraint_name
            WHERE tc.constraint_type = 'FOREIGN KEY'
                AND ccu.table_schema = $1
                AND ccu.table_name = $2
        "#;
        
        let rows = client.query(query, &[&schema, &table]).await?;
        
        for row in rows {
            let dep_schema: String = row.get("table_schema");
            let dep_table: String = row.get("table_name");
            
            impacts.push(DownstreamImpact {
                object_type: "table".to_string(),
                object_name: format!("{}.{}", dep_schema, dep_table),
                impact_type: "foreign_key".to_string(),
                description: format!("Has foreign key referencing {}.{}", schema, table),
            });
        }
        
        // Find views depending on this table
        let view_query = r#"
            SELECT DISTINCT v.table_schema, v.table_name
            FROM information_schema.view_column_usage v
            WHERE v.table_schema = $1 AND v.table_name = $2
        "#;
        
        let view_rows = client.query(view_query, &[&schema, &table]).await?;
        
        for row in view_rows {
            let view_schema: String = row.get("table_schema");
            let view_name: String = row.get("table_name");
            
            impacts.push(DownstreamImpact {
                object_type: "view".to_string(),
                object_name: format!("{}.{}", view_schema, view_name),
                impact_type: "dependency".to_string(),
                description: format!("View depends on {}.{}", schema, table),
            });
        }
        
        Ok(impacts)
    }

    fn calculate_risk_score(
        factors: &[RiskFactor],
        locked_tables: &[String],
        changes: &[SchemaChange],
    ) -> u8 {
        let mut score: u32 = 0;
        
        // Factor-based scoring
        for factor in factors {
            score += match factor.severity {
                RiskLevel::Low => 5,
                RiskLevel::Medium => 15,
                RiskLevel::High => 30,
                RiskLevel::Critical => 50,
            };
        }
        
        // Locked tables penalty
        score += (locked_tables.len() as u32) * 10;
        
        // Destructive changes penalty
        let destructive_count = changes.iter().filter(|c| c.is_destructive()).count();
        score += (destructive_count as u32) * 20;
        
        // Cap at 100
        std::cmp::min(score, 100) as u8
    }

    fn score_to_level(score: u8) -> RiskLevel {
        match score {
            0..=25 => RiskLevel::Low,
            26..=50 => RiskLevel::Medium,
            51..=75 => RiskLevel::High,
            _ => RiskLevel::Critical,
        }
    }

    fn generate_recommendations(
        factors: &[RiskFactor],
        locked_tables: &[String],
        changes: &[SchemaChange],
    ) -> Vec<String> {
        let mut recs = Vec::new();
        
        // Check for critical factors
        if factors.iter().any(|f| f.severity == RiskLevel::Critical) {
            recs.push("⚠️ This migration contains critical changes. Review carefully before proceeding.".to_string());
        }
        
        // Check for data loss
        if factors.iter().any(|f| f.category == "Data Loss") {
            recs.push("💾 Backup affected tables before executing this migration.".to_string());
        }
        
        // Check for table locks
        if !locked_tables.is_empty() {
            recs.push(format!(
                "🔒 Tables {} will be locked during migration. Schedule during low-traffic periods.",
                locked_tables.join(", ")
            ));
        }
        
        // Check for non-concurrent indexes
        if changes.iter().any(|c| matches!(c, SchemaChange::AddIndex(i) if !i.concurrent)) {
            recs.push("📊 Consider using CONCURRENTLY option for index creation to avoid blocking writes.".to_string());
        }
        
        // Check for multiple changes
        if changes.len() > 5 {
            recs.push("📦 Consider breaking this migration into smaller, incremental changes.".to_string());
        }
        
        recs
    }
}
