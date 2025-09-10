import express from "express";
import { Pool } from "pg";
import format from "pg-format";
import dotenv from "dotenv";
import pool from "./utils/database.js";

dotenv.config();
const app = express();
const PORT = process.env.PORT || 3000;

app.use(express.json());

let currentPool = null;

app.post("/createDatabase", async (req, res) => {
  const { Name } = req.body;
  if (!Name) {
    return res.status(400).json({ message: "Database name is required." });
  }

  try {
    const client = await pool.connect();
    const safeQuery = format("CREATE DATABASE %I", Name);
    await client.query(safeQuery);
    client.release();

    console.log(`Database '${Name}' created successfully.`);
    res.status(200).json({ message: `Database '${Name}' created successfully.` });
  } catch (err) {
    console.error(err);
    res.status(500).json({ message: `Error creating database '${Name}'.`, error: err.message });
  }
});

app.get("/listDatabases", async (req, res) => {
  try {
    const client = await pool.connect();
    const result = await client.query(`SELECT datname FROM pg_database WHERE datistemplate = false`);
    client.release();

    const databases = result.rows.map(row => row.datname);
    res.status(200).json({ databases, message: "Databases listed successfully." });
  } catch (err) {
    console.error("Error listing databases:", err);
    res.status(500).json({ message: "Failed to list databases", error: err.message });
  }
});

// update so that you can create not only on your local database but also on the cloud.......
app.post("/connectDatabase", async (req, res) => {
  const { dbName, user, password, host, port } = req.body;

  if (!dbName) {
    return res.status(400).json({ message: "Database name is required." });
  }

  const tempPool = new Pool({
    user: user || process.env.DB_USER,
    host: host || process.env.DB_HOST,
    database: dbName,
    password: password || process.env.DB_PASSWORD,
    port: port || process.env.DB_PORT,
  });

  try {
    const client = await tempPool.connect();
    await client.query("SELECT NOW()");
    client.release();

    currentPool = tempPool;
    res.status(200).json({ message: `Successfully connected to '${dbName}'.` });
  } catch (err) {
    console.error("Connection error:", err);
    res.status(500).json({ message: `Failed to connect to '${dbName}'.`, error: err.message });
  }
});

app.post("/createTable", async (req, res) => {
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

app.get("/listTables", async (req, res) => {
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

app.get("/listColumns", async (req, res) => {
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

    // ✅ normalize to { name, type, nullable }
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


app.post("/deleteDatabase", async (req, res) => {
  const { databaseName } = req.body;

  if (!databaseName) {
    return res.status(400).json({ message: "Database name is required to delete." });
  }

  try {
    const client = await pool.connect();
    const safeQuery = format("DROP DATABASE %I", databaseName);
    await client.query(safeQuery);
    client.release();

    console.log(`Database '${databaseName}' deleted.`);
    res.status(200).json({ message: `Database '${databaseName}' deleted successfully.` });
  } catch (err) {
    console.error(`Error deleting database '${databaseName}':`, err);
    res.status(500).json({ message: `Error deleting database '${databaseName}'.`, error: err.message });
  }
});

app.get("/health", (req, res) => {
  res.status(200).json({ message: "Server is running fine." });
});

// Needs work please check, and we can commit it and call rollback too.  but for a secific section we have to use savepoint and then rollback.
app.post("/autoCommitOff", async (req, res) => {
  try{
    const client = await pool.connect();
    const result = client.query(`\set AUTOCOMMIT OFF`);
    console.log("the query data will be", result);
    res.status(200).json({message: result});
    client.release();
  }
  catch(err){
    console.log("Error recieved",err)
    res.status(500).json({message: err});
  }
})

const startServer = async () => {
  try {
    const client = await pool.connect();
    client.release();

    console.log("Connected to PostgreSQL successfully.");
    app.listen(PORT, () => {
      console.log(`Server is listening on port ${PORT}`);
    });
  } catch (err) {
    console.error("Failed to connect to PostgreSQL:", err);
  }
};

startServer();
