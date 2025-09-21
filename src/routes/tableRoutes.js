import express from "express";
import format from "pg-format";
import { currentPool } from "./databaseRoute.js";

const router = express.Router();

router.post("/create", async (req, res) => {
  const { tableName, columns } = req.body;

  if (!currentPool) {
    return res.status(400).json({ message: "No database connected. Please connect first." });
  }
  if (!tableName || !columns || !Array.isArray(columns)) {
    return res.status(400).json({ message: "tableName and columns (array) are required." });
  }

  try {
    const columnDefs = columns
      .map(col => `${format.ident(col.name)} ${col.type}`)
      .join(", ");
    const query = format("CREATE TABLE %I (%s)", tableName, columnDefs);

    const client = await currentPool.connect();
    await client.query(query);
    client.release();

    res.status(200).json({ message: `Table '${tableName}' created successfully.` });
  } catch (err) {
    console.error("Table creation error:", err);
    res.status(500).json({ message: `Error creating table '${tableName}'`, error: err.message });
  }
});

router.get("/list", async (req, res) => {
  try {
    if (!currentPool) {
      return res.status(400).json({ message: "No database connected. Please connect first." });
    }

    const client = await currentPool.connect();
    const result = await client.query(`
      SELECT 
        n.nspname AS "Schema",
        c.relname AS "Name",
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
        END AS "Type",
        pg_catalog.pg_get_userbyid(c.relowner) AS "Owner"
      FROM pg_catalog.pg_class c
        LEFT JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
      WHERE c.relkind IN ('r','p')
        AND n.nspname <> 'pg_catalog'
        AND n.nspname !~ '^pg_toast'
        AND n.nspname <> 'information_schema'
        AND pg_catalog.pg_table_is_visible(c.oid)
      ORDER BY 1,2;
    `);
    client.release();

    const tables = result.rows.map(row => row.Name);
    res.status(200).json({ tables, message: "The tables are fetched successfully" });
  } catch (err) {
    console.error(err);
    res.status(500).json({ message: err.message });
  }
});

router.get("/columns", async (req, res) => {
  try {
    if (!currentPool) {
      return res.status(400).json({ message: "No database is connected for the request" });
    }

    const { tableName } = req.query;
    if (!tableName) {
      return res.status(400).json({ message: "Table name is required" });
    }

    const client = await currentPool.connect();
    const result = await client.query(
      `
      SELECT column_name, data_type, is_nullable
      FROM information_schema.columns
      WHERE table_schema = 'public' AND table_name = $1
      `,
      [tableName]
    );
    client.release();

    const columns = result.rows.map((col) => ({
      name: col.column_name,
      type: col.data_type,
      nullable: col.is_nullable,
    }));

    res.status(200).json({ columns, message: "Columns fetched successfully" });
  } catch (err) {
    console.error("Error fetching columns:", err);
    res.status(500).json({ message: err.message });
  }
});

export default router;