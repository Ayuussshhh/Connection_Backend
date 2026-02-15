//! Stage One: The Mirror (Enhanced Introspection)
//!
//! Crawls the database to build a comprehensive Semantic Map.
//! This goes beyond basic schema introspection to capture:
//! - Table statistics (row counts, sizes)
//! - Dependencies (views, functions, triggers)
//! - Usage patterns and hot spots
//! - Full "Digital Twin" for the canvas

use crate::error::AppError;
use crate::introspection::SchemaSnapshot;
use crate::pipeline::types::{DependentObject, DependentObjectType, DependencyType, TableStatistics};
use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};
use uuid::Uuid;

// =============================================================================
// SEMANTIC MAP - The "Digital Twin"
// =============================================================================

/// Complete semantic map of a database - the "Digital Twin"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticMap {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub captured_at: DateTime<Utc>,
    
    /// Full schema snapshot
    pub schema: SchemaSnapshot,
    
    /// Table statistics for risk analysis
    pub statistics: HashMap<String, TableStatistics>,
    
    /// Dependency graph (what depends on what)
    pub dependencies: HashMap<String, Vec<DependentObject>>,
    
    /// Hot spots - frequently accessed tables (if pg_stat available)
    pub hot_spots: Vec<HotSpot>,
    
    /// Database-level metadata
    pub db_metadata: DatabaseMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotSpot {
    pub schema: String,
    pub table_name: String,
    pub seq_scans: i64,
    pub idx_scans: i64,
    pub n_tup_ins: i64,
    pub n_tup_upd: i64,
    pub n_tup_del: i64,
    pub activity_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseMetadata {
    pub db_name: String,
    pub db_version: String,
    pub total_size_bytes: i64,
    pub table_count: usize,
    pub index_count: usize,
    pub foreign_key_count: usize,
    pub pg_version: String,
    pub encoding: String,
    pub collation: String,
}

// =============================================================================
// MIRROR SERVICE
// =============================================================================

/// The Mirror - builds and maintains the Semantic Map
pub struct MirrorService;

impl MirrorService {
    /// Build a complete Semantic Map ("Digital Twin") of the database
    pub async fn build_semantic_map(
        pool: &Pool,
        connection_id: Uuid,
        schema: SchemaSnapshot,
    ) -> Result<SemanticMap, AppError> {
        info!("🪞 Building Semantic Map for connection {}", connection_id);
        
        let client = pool.get().await?;
        
        // Gather table statistics
        let statistics = Self::gather_statistics(&client).await?;
        
        // Build dependency graph
        let dependencies = Self::build_dependency_graph(&client).await?;
        
        // Find hot spots
        let hot_spots = Self::find_hot_spots(&client).await?;
        
        // Get database metadata
        let db_metadata = Self::get_database_metadata(&client).await?;
        
        let semantic_map = SemanticMap {
            id: Uuid::new_v4(),
            connection_id,
            captured_at: Utc::now(),
            schema,
            statistics,
            dependencies,
            hot_spots,
            db_metadata,
        };
        
        info!(
            "🪞 Semantic Map built: {} tables, {} statistics, {} dependency chains",
            semantic_map.schema.tables.len(),
            semantic_map.statistics.len(),
            semantic_map.dependencies.len()
        );
        
        Ok(semantic_map)
    }
    
    /// Gather table statistics for risk analysis
    async fn gather_statistics(
        client: &deadpool_postgres::Client,
    ) -> Result<HashMap<String, TableStatistics>, AppError> {
        let query = r#"
            SELECT
                n.nspname as schema_name,
                c.relname as table_name,
                COALESCE(c.reltuples::bigint, 0) as row_count,
                pg_table_size(c.oid) as table_size_bytes,
                pg_indexes_size(c.oid) as index_size_bytes,
                pg_total_relation_size(c.oid) as total_size_bytes,
                s.last_vacuum,
                s.last_analyze,
                c.relispartition as is_partitioned,
                EXISTS(SELECT 1 FROM pg_trigger t WHERE t.tgrelid = c.oid AND NOT t.tgisinternal) as has_triggers,
                COALESCE(s.n_dead_tup, 0) as dead_tuples
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            LEFT JOIN pg_stat_user_tables s ON s.relid = c.oid
            WHERE c.relkind = 'r'
              AND n.nspname NOT IN ('pg_catalog', 'information_schema')
            ORDER BY n.nspname, c.relname
        "#;
        
        let rows = client.query(query, &[]).await?;
        let mut statistics = HashMap::new();
        
        for row in rows {
            let schema: String = row.get("schema_name");
            let table: String = row.get("table_name");
            let key = format!("{}.{}", schema, table);
            
            statistics.insert(key, TableStatistics {
                schema,
                table_name: table,
                row_count: row.get("row_count"),
                table_size_bytes: row.get("table_size_bytes"),
                index_size_bytes: row.get("index_size_bytes"),
                total_size_bytes: row.get("total_size_bytes"),
                last_vacuum: row.get("last_vacuum"),
                last_analyze: row.get("last_analyze"),
                is_partitioned: row.get("is_partitioned"),
                has_triggers: row.get("has_triggers"),
                dead_tuples: row.get("dead_tuples"),
            });
        }
        
        debug!("📊 Gathered statistics for {} tables", statistics.len());
        Ok(statistics)
    }
    
    /// Build dependency graph - what depends on what
    async fn build_dependency_graph(
        client: &deadpool_postgres::Client,
    ) -> Result<HashMap<String, Vec<DependentObject>>, AppError> {
        let mut dependencies: HashMap<String, Vec<DependentObject>> = HashMap::new();
        
        // Get views that depend on tables
        let view_query = r#"
            SELECT DISTINCT
                source_ns.nspname as source_schema,
                source_table.relname as source_table,
                view_ns.nspname as view_schema,
                view_rel.relname as view_name,
                CASE WHEN view_rel.relkind = 'v' THEN 'view' ELSE 'materialized_view' END as view_type
            FROM pg_depend d
            JOIN pg_rewrite r ON r.oid = d.objid
            JOIN pg_class view_rel ON r.ev_class = view_rel.oid
            JOIN pg_class source_table ON d.refobjid = source_table.oid
            JOIN pg_namespace view_ns ON view_rel.relnamespace = view_ns.oid
            JOIN pg_namespace source_ns ON source_table.relnamespace = source_ns.oid
            WHERE source_table.relkind = 'r'
              AND view_rel.relkind IN ('v', 'm')
              AND source_ns.nspname NOT IN ('pg_catalog', 'information_schema')
              AND view_ns.nspname NOT IN ('pg_catalog', 'information_schema')
              AND source_table.relname != view_rel.relname
        "#;
        
        let view_rows = client.query(view_query, &[]).await?;
        for row in view_rows {
            let source_schema: String = row.get("source_schema");
            let source_table: String = row.get("source_table");
            let key = format!("{}.{}", source_schema, source_table);
            
            let view_type: String = row.get("view_type");
            let obj_type = if view_type == "view" {
                DependentObjectType::View
            } else {
                DependentObjectType::MaterializedView
            };
            
            dependencies.entry(key).or_default().push(DependentObject {
                object_type: obj_type,
                schema: row.get("view_schema"),
                name: row.get("view_name"),
                dependency_type: DependencyType::Hard,
                details: None,
            });
        }
        
        // Get functions that reference tables
        let func_query = r#"
            SELECT DISTINCT
                n.nspname as schema_name,
                p.proname as func_name,
                d.refobjid::regclass::text as table_ref
            FROM pg_depend d
            JOIN pg_proc p ON d.objid = p.oid
            JOIN pg_namespace n ON p.pronamespace = n.oid
            WHERE d.classid = 'pg_proc'::regclass
              AND d.refclassid = 'pg_class'::regclass
              AND n.nspname NOT IN ('pg_catalog', 'information_schema')
        "#;
        
        if let Ok(func_rows) = client.query(func_query, &[]).await {
            for row in func_rows {
                let table_ref: String = row.get("table_ref");
                
                dependencies.entry(table_ref).or_default().push(DependentObject {
                    object_type: DependentObjectType::Function,
                    schema: row.get("schema_name"),
                    name: row.get("func_name"),
                    dependency_type: DependencyType::Soft,
                    details: None,
                });
            }
        }
        
        // Get triggers
        let trigger_query = r#"
            SELECT
                n.nspname as schema_name,
                c.relname as table_name,
                t.tgname as trigger_name,
                p.proname as function_name
            FROM pg_trigger t
            JOIN pg_class c ON t.tgrelid = c.oid
            JOIN pg_namespace n ON c.relnamespace = n.oid
            JOIN pg_proc p ON t.tgfoid = p.oid
            WHERE NOT t.tgisinternal
              AND n.nspname NOT IN ('pg_catalog', 'information_schema')
        "#;
        
        if let Ok(trigger_rows) = client.query(trigger_query, &[]).await {
            for row in trigger_rows {
                let schema: String = row.get("schema_name");
                let table: String = row.get("table_name");
                let key = format!("{}.{}", schema, table);
                
                dependencies.entry(key).or_default().push(DependentObject {
                    object_type: DependentObjectType::Trigger,
                    schema,
                    name: row.get("trigger_name"),
                    dependency_type: DependencyType::Cascade,
                    details: Some(format!("Calls function: {}", row.get::<_, String>("function_name"))),
                });
            }
        }
        
        debug!("🔗 Built dependency graph for {} objects", dependencies.len());
        Ok(dependencies)
    }
    
    /// Find hot spots - tables with high activity
    async fn find_hot_spots(
        client: &deadpool_postgres::Client,
    ) -> Result<Vec<HotSpot>, AppError> {
        let query = r#"
            SELECT
                schemaname as schema_name,
                relname as table_name,
                COALESCE(seq_scan, 0) as seq_scans,
                COALESCE(idx_scan, 0) as idx_scans,
                COALESCE(n_tup_ins, 0) as n_tup_ins,
                COALESCE(n_tup_upd, 0) as n_tup_upd,
                COALESCE(n_tup_del, 0) as n_tup_del
            FROM pg_stat_user_tables
            WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
            ORDER BY (COALESCE(seq_scan, 0) + COALESCE(idx_scan, 0) + 
                      COALESCE(n_tup_ins, 0) + COALESCE(n_tup_upd, 0) + 
                      COALESCE(n_tup_del, 0)) DESC
            LIMIT 20
        "#;
        
        let rows = client.query(query, &[]).await?;
        let mut hot_spots = Vec::new();
        
        for row in rows {
            let seq_scans: i64 = row.get("seq_scans");
            let idx_scans: i64 = row.get("idx_scans");
            let n_tup_ins: i64 = row.get("n_tup_ins");
            let n_tup_upd: i64 = row.get("n_tup_upd");
            let n_tup_del: i64 = row.get("n_tup_del");
            
            // Calculate activity score (weighted)
            let activity_score = (seq_scans as f64 * 0.5)
                + (idx_scans as f64 * 1.0)
                + (n_tup_ins as f64 * 2.0)
                + (n_tup_upd as f64 * 3.0)
                + (n_tup_del as f64 * 2.5);
            
            hot_spots.push(HotSpot {
                schema: row.get("schema_name"),
                table_name: row.get("table_name"),
                seq_scans,
                idx_scans,
                n_tup_ins,
                n_tup_upd,
                n_tup_del,
                activity_score,
            });
        }
        
        debug!("🔥 Found {} hot spots", hot_spots.len());
        Ok(hot_spots)
    }
    
    /// Get database-level metadata
    async fn get_database_metadata(
        client: &deadpool_postgres::Client,
    ) -> Result<DatabaseMetadata, AppError> {
        // Get database name and version
        let db_info_query = r#"
            SELECT
                current_database() as db_name,
                version() as db_version,
                pg_database_size(current_database()) as total_size,
                pg_encoding_to_char(encoding) as encoding,
                datcollate as collation
            FROM pg_database
            WHERE datname = current_database()
        "#;
        
        let row = client.query_one(db_info_query, &[]).await?;
        
        // Count objects
        let count_query = r#"
            SELECT
                (SELECT COUNT(*) FROM pg_class c 
                 JOIN pg_namespace n ON c.relnamespace = n.oid 
                 WHERE c.relkind = 'r' AND n.nspname NOT IN ('pg_catalog', 'information_schema')) as table_count,
                (SELECT COUNT(*) FROM pg_class c 
                 JOIN pg_namespace n ON c.relnamespace = n.oid 
                 WHERE c.relkind = 'i' AND n.nspname NOT IN ('pg_catalog', 'information_schema')) as index_count,
                (SELECT COUNT(*) FROM information_schema.table_constraints 
                 WHERE constraint_type = 'FOREIGN KEY') as fk_count
        "#;
        
        let counts = client.query_one(count_query, &[]).await?;
        
        // Extract PostgreSQL version
        let version_str: String = row.get("db_version");
        let pg_version = version_str
            .split_whitespace()
            .nth(1)
            .unwrap_or("unknown")
            .to_string();
        
        Ok(DatabaseMetadata {
            db_name: row.get("db_name"),
            db_version: version_str,
            total_size_bytes: row.get("total_size"),
            table_count: counts.get::<_, i64>("table_count") as usize,
            index_count: counts.get::<_, i64>("index_count") as usize,
            foreign_key_count: counts.get::<_, i64>("fk_count") as usize,
            pg_version,
            encoding: row.get("encoding"),
            collation: row.get("collation"),
        })
    }
    
    /// Check for drift between stored snapshot and live database
    pub async fn check_drift(
        pool: &Pool,
        connection_id: Uuid,
        stored_checksum: &str,
    ) -> Result<DriftCheckResult, AppError> {
        use crate::introspection::PostgresIntrospector;
        
        // Get fresh schema
        let live_schema = PostgresIntrospector::introspect(pool, connection_id).await?;
        
        let has_drift = live_schema.checksum != stored_checksum;
        
        Ok(DriftCheckResult {
            has_drift,
            stored_checksum: stored_checksum.to_string(),
            live_checksum: live_schema.checksum.clone(),
            checked_at: Utc::now(),
            live_schema: if has_drift { Some(live_schema) } else { None },
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftCheckResult {
    pub has_drift: bool,
    pub stored_checksum: String,
    pub live_checksum: String,
    pub checked_at: DateTime<Utc>,
    /// The live schema (only included if drift detected)
    pub live_schema: Option<SchemaSnapshot>,
}
