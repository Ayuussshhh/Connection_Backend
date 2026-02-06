//! SQL query constants and builders
//!
//! Contains all SQL queries used by the application.

/// List all non-template databases
pub const LIST_DATABASES: &str = r#"
    SELECT datname 
    FROM pg_database 
    WHERE datistemplate = false
    ORDER BY datname
"#;

/// List all tables in public schema
pub const LIST_TABLES: &str = r#"
    SELECT 
        n.nspname AS schema,
        c.relname AS name,
        CASE c.relkind
            WHEN 'r' THEN 'table'
            WHEN 'v' THEN 'view'
            WHEN 'm' THEN 'materialized view'
            WHEN 'i' THEN 'index'
            WHEN 'S' THEN 'sequence'
            WHEN 't' THEN 'TOAST table'
            WHEN 'f' THEN 'foreign table'
            WHEN 'p' THEN 'partitioned table'
            WHEN 'I' THEN 'partitioned index'
        END AS type,
        pg_catalog.pg_get_userbyid(c.relowner) AS owner
    FROM pg_catalog.pg_class c
        LEFT JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
    WHERE c.relkind IN ('r','p')
        AND n.nspname <> 'pg_catalog'
        AND n.nspname !~ '^pg_toast'
        AND n.nspname <> 'information_schema'
        AND pg_catalog.pg_table_is_visible(c.oid)
    ORDER BY schema, name
"#;

/// Get column information for a table
pub const GET_COLUMNS: &str = r#"
    SELECT 
        c.column_name,
        c.data_type,
        c.is_nullable = 'YES' AS nullable,
        c.column_default,
        c.character_maximum_length,
        COALESCE(pk.is_pk, false) AS is_primary_key,
        COALESCE(uq.is_unique, false) AS is_unique
    FROM information_schema.columns c
    LEFT JOIN (
        SELECT kcu.column_name, true AS is_pk
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu 
            ON tc.constraint_name = kcu.constraint_name
            AND tc.table_schema = kcu.table_schema
        WHERE tc.constraint_type = 'PRIMARY KEY'
            AND tc.table_schema = 'public'
            AND tc.table_name = $1
    ) pk ON c.column_name = pk.column_name
    LEFT JOIN (
        SELECT kcu.column_name, true AS is_unique
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu 
            ON tc.constraint_name = kcu.constraint_name
            AND tc.table_schema = kcu.table_schema
        WHERE tc.constraint_type = 'UNIQUE'
            AND tc.table_schema = 'public'
            AND tc.table_name = $1
    ) uq ON c.column_name = uq.column_name
    WHERE c.table_schema = 'public' 
        AND c.table_name = $1
    ORDER BY c.ordinal_position
"#;

/// Get foreign keys for a specific table
pub const GET_FOREIGN_KEYS: &str = r#"
    SELECT
        tc.constraint_name,
        kcu.column_name,
        ccu.table_name AS referenced_table,
        ccu.column_name AS referenced_column,
        rc.update_rule,
        rc.delete_rule
    FROM information_schema.table_constraints AS tc
    JOIN information_schema.key_column_usage AS kcu
        ON tc.constraint_name = kcu.constraint_name
        AND tc.table_schema = kcu.table_schema
    JOIN information_schema.constraint_column_usage AS ccu
        ON ccu.constraint_name = tc.constraint_name
        AND ccu.table_schema = tc.table_schema
    JOIN information_schema.referential_constraints AS rc
        ON rc.constraint_name = tc.constraint_name
        AND rc.constraint_schema = tc.table_schema
    WHERE tc.table_name = $1 
        AND tc.table_schema = 'public'
        AND tc.constraint_type = 'FOREIGN KEY'
    ORDER BY tc.constraint_name
"#;

/// Get all foreign keys in the database
pub const GET_ALL_FOREIGN_KEYS: &str = r#"
    SELECT
        tc.table_name,
        tc.constraint_name,
        kcu.column_name,
        ccu.table_name AS referenced_table,
        ccu.column_name AS referenced_column,
        rc.update_rule,
        rc.delete_rule
    FROM information_schema.table_constraints AS tc
    JOIN information_schema.key_column_usage AS kcu
        ON tc.constraint_name = kcu.constraint_name
        AND tc.table_schema = kcu.table_schema
    JOIN information_schema.constraint_column_usage AS ccu
        ON ccu.constraint_name = tc.constraint_name
        AND ccu.table_schema = tc.table_schema
    JOIN information_schema.referential_constraints AS rc
        ON rc.constraint_name = tc.constraint_name
        AND rc.constraint_schema = tc.table_schema
    WHERE tc.constraint_type = 'FOREIGN KEY'
        AND tc.table_schema = 'public'
    ORDER BY tc.table_name, tc.constraint_name
"#;

/// Check if a constraint already exists
pub const CHECK_CONSTRAINT_EXISTS: &str = r#"
    SELECT constraint_name 
    FROM information_schema.table_constraints 
    WHERE constraint_name = $1 
        AND constraint_type = 'FOREIGN KEY'
        AND table_schema = 'public'
"#;

/// Get primary keys for a table
pub const GET_PRIMARY_KEYS: &str = r#"
    SELECT kcu.column_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu 
        ON tc.constraint_name = kcu.constraint_name
        AND tc.table_schema = kcu.table_schema
    WHERE tc.constraint_type = 'PRIMARY KEY'
        AND tc.table_schema = 'public'
        AND tc.table_name = $1
    ORDER BY kcu.ordinal_position
"#;

/// Check if a column can be referenced (has unique or primary key constraint)
pub const VALIDATE_REFERENCE: &str = r#"
    SELECT EXISTS(
        SELECT 1 
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu 
            ON tc.constraint_name = kcu.constraint_name
            AND tc.table_schema = kcu.table_schema
        WHERE tc.table_schema = 'public'
            AND tc.table_name = $1
            AND kcu.column_name = $2
            AND tc.constraint_type IN ('PRIMARY KEY', 'UNIQUE')
    ) AS is_valid
"#;

/// SQL builder for safe identifier quoting
pub struct SqlBuilder;

impl SqlBuilder {
    /// Quote an identifier (table/column name) safely
    pub fn quote_ident(ident: &str) -> String {
        // PostgreSQL identifier quoting
        format!("\"{}\"", ident.replace('"', "\"\""))
    }

    /// Build CREATE DATABASE query
    pub fn create_database(name: &str) -> String {
        format!("CREATE DATABASE {}", Self::quote_ident(name))
    }

    /// Build DROP DATABASE query
    pub fn drop_database(name: &str) -> String {
        format!("DROP DATABASE {}", Self::quote_ident(name))
    }

    /// Build CREATE TABLE query
    pub fn create_table(name: &str, column_defs: &str) -> String {
        format!("CREATE TABLE {} ({})", Self::quote_ident(name), column_defs)
    }

    /// Build ALTER TABLE ADD CONSTRAINT query for foreign key
    pub fn add_foreign_key(
        source_table: &str,
        constraint_name: &str,
        source_column: &str,
        referenced_table: &str,
        referenced_column: &str,
        on_delete: &str,
        on_update: &str,
    ) -> String {
        format!(
            "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {}({}) ON DELETE {} ON UPDATE {}",
            Self::quote_ident(source_table),
            Self::quote_ident(constraint_name),
            Self::quote_ident(source_column),
            Self::quote_ident(referenced_table),
            Self::quote_ident(referenced_column),
            on_delete,
            on_update
        )
    }

    /// Build ALTER TABLE DROP CONSTRAINT query
    pub fn drop_constraint(table_name: &str, constraint_name: &str) -> String {
        format!(
            "ALTER TABLE {} DROP CONSTRAINT {}",
            Self::quote_ident(table_name),
            Self::quote_ident(constraint_name)
        )
    }
}
