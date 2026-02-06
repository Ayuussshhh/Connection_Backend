//! HTTP request handlers module
//!
//! This module re-exports all route handlers for convenience.
//! Individual handlers are implemented in the routes module.

// This module serves as a documentation entry point.
// All handlers are implemented directly in the routes module.

// Database handlers:
// - create_database: POST /db/create
// - list_databases: GET /db/list
// - connect_database: POST /db/connect
// - delete_database: POST /db/delete
// - disconnect_database: POST /db/disconnect
// - connection_status: GET /db/status

// Table handlers:
// - create_table: POST /table/create
// - list_tables: GET /table/list
// - get_columns: GET /table/columns

// Foreign key handlers:
// - create_foreign_key: POST /foreignKey/create
// - list_foreign_keys: GET /foreignKey/list
// - list_all_foreign_keys: GET /foreignKey/listAll
// - delete_foreign_key: POST /foreignKey/delete
// - get_primary_keys: GET /foreignKey/primaryKeys
// - validate_reference: POST /foreignKey/validateReference
