//! Migration SQL generator
//!
//! Generates PostgreSQL DDL statements from schema changes.

use crate::proposal::*;

pub struct MigrationGenerator;

impl MigrationGenerator {
    /// Generate forward migration SQL from changes
    pub fn generate_migration(changes: &[SchemaChange]) -> String {
        changes
            .iter()
            .map(Self::change_to_sql)
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Generate rollback SQL from changes
    pub fn generate_rollback(changes: &[SchemaChange]) -> String {
        changes
            .iter()
            .rev()
            .filter_map(Self::change_to_rollback_sql)
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Convert a single change to SQL
    fn change_to_sql(change: &SchemaChange) -> String {
        match change {
            SchemaChange::CreateTable(c) => Self::create_table_sql(c),
            SchemaChange::DropTable(c) => Self::drop_table_sql(c),
            SchemaChange::RenameTable(c) => Self::rename_table_sql(c),
            SchemaChange::AddColumn(c) => Self::add_column_sql(c),
            SchemaChange::DropColumn(c) => Self::drop_column_sql(c),
            SchemaChange::ModifyColumn(c) => Self::modify_column_sql(c),
            SchemaChange::RenameColumn(c) => Self::rename_column_sql(c),
            SchemaChange::AddForeignKey(c) => Self::add_foreign_key_sql(c),
            SchemaChange::DropForeignKey(c) => Self::drop_foreign_key_sql(c),
            SchemaChange::AddIndex(c) => Self::add_index_sql(c),
            SchemaChange::DropIndex(c) => Self::drop_index_sql(c),
        }
    }

    /// Generate rollback SQL for a change (returns None if not reversible)
    fn change_to_rollback_sql(change: &SchemaChange) -> Option<String> {
        match change {
            SchemaChange::CreateTable(c) => Some(format!(
                "DROP TABLE IF EXISTS \"{}\".\"{}\" CASCADE;",
                c.schema, c.table_name
            )),
            SchemaChange::DropTable(_) => None, // Can't rollback a drop without backup
            SchemaChange::RenameTable(c) => Some(format!(
                "ALTER TABLE \"{}\".\"{}\" RENAME TO \"{}\";",
                c.schema, c.new_name, c.old_name
            )),
            SchemaChange::AddColumn(c) => Some(format!(
                "ALTER TABLE \"{}\".\"{}\" DROP COLUMN IF EXISTS \"{}\";",
                c.schema, c.table_name, c.column.name
            )),
            SchemaChange::DropColumn(_) => None, // Can't rollback without data
            SchemaChange::ModifyColumn(_) => None, // Complex rollback needs original state
            SchemaChange::RenameColumn(c) => Some(format!(
                "ALTER TABLE \"{}\".\"{}\" RENAME COLUMN \"{}\" TO \"{}\";",
                c.schema, c.table_name, c.new_name, c.old_name
            )),
            SchemaChange::AddForeignKey(c) => {
                let constraint_name = c.constraint_name.as_ref()
                    .cloned()
                    .unwrap_or_else(|| format!("fk_{}_{}", c.source_table, c.target_table));
                Some(format!(
                    "ALTER TABLE \"{}\".\"{}\" DROP CONSTRAINT IF EXISTS \"{}\";",
                    c.source_schema, c.source_table, constraint_name
                ))
            }
            SchemaChange::DropForeignKey(_) => None, // Can't rollback without definition
            SchemaChange::AddIndex(c) => {
                let index_name = c.index_name.as_ref()
                    .cloned()
                    .unwrap_or_else(|| format!("idx_{}_{}", c.table_name, c.columns.join("_")));
                Some(format!(
                    "DROP INDEX IF EXISTS \"{}\".\"{}\"{}",
                    c.schema, index_name,
                    if c.concurrent { " CONCURRENTLY" } else { "" }
                ))
            }
            SchemaChange::DropIndex(_) => None, // Can't rollback without definition
        }
    }

    fn create_table_sql(c: &CreateTableChange) -> String {
        let columns: Vec<String> = c.columns.iter().map(|col| {
            let mut def = format!("    \"{}\" {}", col.name, col.data_type);
            if !col.nullable {
                def.push_str(" NOT NULL");
            }
            if let Some(ref default) = col.default_value {
                def.push_str(&format!(" DEFAULT {}", default));
            }
            def
        }).collect();

        let mut sql = format!(
            "CREATE TABLE \"{}\".\"{}\" (\n{}\n",
            c.schema, c.table_name, columns.join(",\n")
        );

        if let Some(ref pk) = c.primary_key {
            let pk_cols: Vec<String> = pk.iter().map(|c| format!("\"{}\"", c)).collect();
            sql.push_str(&format!(",\n    PRIMARY KEY ({})\n", pk_cols.join(", ")));
        }

        sql.push_str(");");
        sql
    }

    fn drop_table_sql(c: &DropTableChange) -> String {
        format!(
            "DROP TABLE{} \"{}\".\"{}\"{}",
            "",
            c.schema,
            c.table_name,
            if c.cascade { " CASCADE" } else { "" }
        )
    }

    fn rename_table_sql(c: &RenameTableChange) -> String {
        format!(
            "ALTER TABLE \"{}\".\"{}\" RENAME TO \"{}\";",
            c.schema, c.old_name, c.new_name
        )
    }

    fn add_column_sql(c: &AddColumnChange) -> String {
        let mut sql = format!(
            "ALTER TABLE \"{}\".\"{}\" ADD COLUMN \"{}\" {}",
            c.schema, c.table_name, c.column.name, c.column.data_type
        );
        
        if !c.column.nullable {
            sql.push_str(" NOT NULL");
        }
        
        if let Some(ref default) = c.column.default_value {
            sql.push_str(&format!(" DEFAULT {}", default));
        }
        
        sql.push(';');
        sql
    }

    fn drop_column_sql(c: &DropColumnChange) -> String {
        format!(
            "ALTER TABLE \"{}\".\"{}\" DROP COLUMN \"{}\"{}",
            c.schema, c.table_name, c.column_name,
            if c.cascade { " CASCADE" } else { "" }
        )
    }

    fn modify_column_sql(c: &ModifyColumnChange) -> String {
        let mut statements = Vec::new();
        
        if let Some(ref new_type) = c.new_type {
            statements.push(format!(
                "ALTER TABLE \"{}\".\"{}\" ALTER COLUMN \"{}\" TYPE {} USING \"{}\"::{};",
                c.schema, c.table_name, c.column_name, new_type, c.column_name, new_type
            ));
        }
        
        if let Some(nullable) = c.new_nullable {
            if nullable {
                statements.push(format!(
                    "ALTER TABLE \"{}\".\"{}\" ALTER COLUMN \"{}\" DROP NOT NULL;",
                    c.schema, c.table_name, c.column_name
                ));
            } else {
                statements.push(format!(
                    "ALTER TABLE \"{}\".\"{}\" ALTER COLUMN \"{}\" SET NOT NULL;",
                    c.schema, c.table_name, c.column_name
                ));
            }
        }
        
        if let Some(ref new_default) = c.new_default {
            if new_default == "NULL" || new_default.is_empty() {
                statements.push(format!(
                    "ALTER TABLE \"{}\".\"{}\" ALTER COLUMN \"{}\" DROP DEFAULT;",
                    c.schema, c.table_name, c.column_name
                ));
            } else {
                statements.push(format!(
                    "ALTER TABLE \"{}\".\"{}\" ALTER COLUMN \"{}\" SET DEFAULT {};",
                    c.schema, c.table_name, c.column_name, new_default
                ));
            }
        }
        
        statements.join("\n")
    }

    fn rename_column_sql(c: &RenameColumnChange) -> String {
        format!(
            "ALTER TABLE \"{}\".\"{}\" RENAME COLUMN \"{}\" TO \"{}\";",
            c.schema, c.table_name, c.old_name, c.new_name
        )
    }

    fn add_foreign_key_sql(c: &AddForeignKeyChange) -> String {
        let constraint_name = c.constraint_name.as_ref()
            .cloned()
            .unwrap_or_else(|| format!("fk_{}_{}", c.source_table, c.target_table));
        
        let source_cols: Vec<String> = c.source_columns.iter().map(|c| format!("\"{}\"", c)).collect();
        let target_cols: Vec<String> = c.target_columns.iter().map(|c| format!("\"{}\"", c)).collect();
        
        let mut sql = format!(
            "ALTER TABLE \"{}\".\"{}\" ADD CONSTRAINT \"{}\" FOREIGN KEY ({}) REFERENCES \"{}\".\"{}\" ({})",
            c.source_schema, c.source_table, constraint_name,
            source_cols.join(", "),
            c.target_schema, c.target_table,
            target_cols.join(", ")
        );
        
        if let Some(ref on_delete) = c.on_delete {
            sql.push_str(&format!(" ON DELETE {}", on_delete));
        }
        
        if let Some(ref on_update) = c.on_update {
            sql.push_str(&format!(" ON UPDATE {}", on_update));
        }
        
        sql.push(';');
        sql
    }

    fn drop_foreign_key_sql(c: &DropForeignKeyChange) -> String {
        format!(
            "ALTER TABLE \"{}\".\"{}\" DROP CONSTRAINT \"{}\";",
            c.schema, c.table_name, c.constraint_name
        )
    }

    fn add_index_sql(c: &AddIndexChange) -> String {
        let index_name = c.index_name.as_ref()
            .cloned()
            .unwrap_or_else(|| format!("idx_{}_{}", c.table_name, c.columns.join("_")));
        
        let cols: Vec<String> = c.columns.iter().map(|col| format!("\"{}\"", col)).collect();
        
        format!(
            "CREATE {}INDEX{} \"{}\" ON \"{}\".\"{}\" ({});",
            if c.unique { "UNIQUE " } else { "" },
            if c.concurrent { " CONCURRENTLY" } else { "" },
            index_name, c.schema, c.table_name, cols.join(", ")
        )
    }

    fn drop_index_sql(c: &DropIndexChange) -> String {
        format!(
            "DROP INDEX{} \"{}\".\"{}\"{}",
            if c.concurrent { " CONCURRENTLY" } else { "" },
            c.schema, c.index_name,
            ""
        )
    }
}
