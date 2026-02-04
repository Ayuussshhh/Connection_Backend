import express from "express";
import format from "pg-format";
import { currentPool } from "./databaseRoute.js";

const router = express.Router();

// Create a foreign key constraint
router.post("/create", async (req, res) => {
  const {
    sourceTable,
    sourceColumn,
    referencedTable,
    referencedColumn,
    constraintName,
    onDelete = "RESTRICT",
    onUpdate = "RESTRICT",
  } = req.body;

  if (!sourceTable || !sourceColumn || !referencedTable || !referencedColumn) {
    console.log("There is an error in creating the foreign key relation", sourceTable, sourceColumn, referencedTable, referencedColumn)
    return res
      .status(400)
      .json({
        message:
          "sourceTable, sourceColumn, referencedTable, and referencedColumn are required.",
      });
  }

  if (!currentPool) {
    return res
      .status(400)
      .json({ message: "No database connected. Please connect first." });
  }

  const allowedActions = ["RESTRICT", "CASCADE", "SET NULL", "NO ACTION", "SET DEFAULT"];
  if (!allowedActions.includes(onDelete) || !allowedActions.includes(onUpdate)) {
    return res
      .status(400)
      .json({
        message: `onDelete and onUpdate must be one of: ${allowedActions.join(", ")}`,
      });
  }

  try {
    const client = await currentPool.connect();

    // Generate constraint name if not provided
    const fkName =
      constraintName ||
      `fk_${sourceTable}_${sourceColumn}_${referencedTable}_${referencedColumn}`;

    // Check if constraint already exists
    const checkConstraint = await client.query(
      `
      SELECT constraint_name 
      FROM information_schema.table_constraints 
      WHERE constraint_name = $1 AND constraint_type = 'FOREIGN KEY'
      `,
      [fkName]
    );

    if (checkConstraint.rows.length > 0) {
      client.release();
      return res
        .status(400)
        .json({ message: `Constraint '${fkName}' already exists.` });
    }

    // Create the foreign key
    const createFKQuery = `
      ALTER TABLE ${format.ident(sourceTable)}
      ADD CONSTRAINT ${format.ident(fkName)}
      FOREIGN KEY (${format.ident(sourceColumn)})
      REFERENCES ${format.ident(referencedTable)}(${format.ident(referencedColumn)})
      ON DELETE ${onDelete}
      ON UPDATE ${onUpdate}
    `;

    await client.query(createFKQuery);
    client.release();

    res.status(200).json({
      message: `Foreign key constraint '${fkName}' created successfully.`,
      constraint: {
        name: fkName,
        sourceTable,
        sourceColumn,
        referencedTable,
        referencedColumn,
        onDelete,
        onUpdate,
      },
    });
  } catch (err) {
    console.error("Foreign key creation error:", err);
    res
      .status(500)
      .json({
        message: "Error creating foreign key constraint",
        error: err.message,
      });
  }
});

// List all foreign keys for a table
router.get("/list", async (req, res) => {
  const { tableName } = req.query;

  if (!tableName) {
    return res.status(400).json({ message: "Table name is required" });
  }

  if (!currentPool) {
    return res
      .status(400)
      .json({ message: "No database connected. Please connect first." });
  }

  try {
    const client = await currentPool.connect();

    const result = await client.query(
      `
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
      JOIN information_schema.constraint_column_usage AS ccu
        ON ccu.constraint_name = tc.constraint_name
      JOIN information_schema.referential_constraints AS rc
        ON rc.constraint_name = tc.constraint_name
      WHERE tc.table_name = $1 AND tc.constraint_type = 'FOREIGN KEY'
      ORDER BY tc.constraint_name
      `,
      [tableName]
    );

    client.release();

    const foreignKeys = result.rows.map((row) => ({
      name: row.constraint_name,
      column: row.column_name,
      referencedTable: row.referenced_table,
      referencedColumn: row.referenced_column,
      onUpdate: row.update_rule,
      onDelete: row.delete_rule,
    }));

    res.status(200).json({
      foreignKeys,
      message: "Foreign keys fetched successfully",
    });
  } catch (err) {
    console.error("Error fetching foreign keys:", err);
    res.status(500).json({ message: err.message });
  }
});

// List all foreign keys for the entire database (across all tables)
router.get("/listAll", async (req, res) => {
  if (!currentPool) {
    return res
      .status(400)
      .json({ message: "No database connected. Please connect first." });
  }

  try {
    const client = await currentPool.connect();

    const result = await client.query(`
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
      JOIN information_schema.constraint_column_usage AS ccu
        ON ccu.constraint_name = tc.constraint_name
      JOIN information_schema.referential_constraints AS rc
        ON rc.constraint_name = tc.constraint_name
      WHERE tc.constraint_type = 'FOREIGN KEY'
        AND tc.table_schema = 'public'
      ORDER BY tc.table_name, tc.constraint_name
    `);

    client.release();

    const foreignKeys = result.rows.map((row) => ({
      sourceTable: row.table_name,
      name: row.constraint_name,
      column: row.column_name,
      referencedTable: row.referenced_table,
      referencedColumn: row.referenced_column,
      onUpdate: row.update_rule,
      onDelete: row.delete_rule,
    }));

    res.status(200).json({
      foreignKeys,
      message: "All foreign keys fetched successfully",
    });
  } catch (err) {
    console.error("Error fetching foreign keys:", err);
    res.status(500).json({ message: err.message });
  }
});

// Delete a foreign key constraint
router.post("/delete", async (req, res) => {
  const { tableName, constraintName } = req.body;

  if (!tableName || !constraintName) {
    return res
      .status(400)
      .json({
        message: "tableName and constraintName are required.",
      });
  }

  if (!currentPool) {
    return res
      .status(400)
      .json({ message: "No database connected. Please connect first." });
  }

  try {
    const client = await currentPool.connect();

    const dropFKQuery = `
      ALTER TABLE ${format.ident(tableName)}
      DROP CONSTRAINT ${format.ident(constraintName)}
    `;

    await client.query(dropFKQuery);
    client.release();

    res.status(200).json({
      message: `Foreign key constraint '${constraintName}' deleted successfully.`,
    });
  } catch (err) {
    console.error("Foreign key deletion error:", err);
    res
      .status(500)
      .json({
        message: "Error deleting foreign key constraint",
        error: err.message,
      });
  }
});

// Get primary keys for a table
router.get("/primaryKeys", async (req, res) => {
  const { tableName } = req.query;

  if (!tableName) {
    return res.status(400).json({ message: "Table name is required" });
  }

  if (!currentPool) {
    return res
      .status(400)
      .json({ message: "No database connected. Please connect first." });
  }

  try {
    const client = await currentPool.connect();

    const result = await client.query(
      `
      SELECT a.attname AS column_name
      FROM pg_index i
      JOIN pg_attribute a ON a.attrelid = i.indrelid
        AND a.attnum = ANY(i.indkey)
      WHERE i.indrelname = $1
        AND i.indisprimary = true
      `,
      [tableName]
    );

    client.release();

    const primaryKeys = result.rows.map((row) => row.column_name);

    res.status(200).json({
      primaryKeys,
      message: "Primary keys fetched successfully",
    });
  } catch (err) {
    console.error("Error fetching primary keys:", err);
    res.status(500).json({ message: err.message });
  }
});

// Validate if a column can be referenced as a foreign key
router.post("/validateReference", async (req, res) => {
  const { tableName, columnName } = req.body;

  if (!tableName || !columnName) {
    return res
      .status(400)
      .json({ message: "tableName and columnName are required." });
  }

  if (!currentPool) {
    return res
      .status(400)
      .json({ message: "No database connected. Please connect first." });
  }

  try {
    const client = await currentPool.connect();

    // Check if column has a unique constraint (primary key or unique)
    const result = await client.query(
      `
      SELECT EXISTS(
        SELECT 1 FROM pg_index
        WHERE indrelname = $1
          AND $2::name = ANY(
            SELECT attname FROM pg_attribute
            WHERE attrelid = (SELECT oid FROM pg_class WHERE relname = $1)
              AND attnum = ANY(
                SELECT i.indkey::int2[] 
                FROM pg_index i 
                WHERE i.indrelname = $1 
                  AND (i.indisprimary OR i.indisunique)
              )
          )
      ) AS is_valid_reference
      `,
      [tableName, columnName]
    );

    const isValid = result.rows[0].is_valid_reference;
    client.release();

    res.status(200).json({
      isValid,
      message: isValid
        ? "Column can be referenced as a foreign key"
        : "Column cannot be referenced (must be primary key or unique)",
    });
  } catch (err) {
    console.error("Error validating reference:", err);
    res
      .status(500)
      .json({
        message: "Error validating reference",
        error: err.message,
      });
  }
});

export default router;
