import express from "express";
import { Pool } from "pg";
import format from "pg-format";
import pool from "../utils/database.js";

const router = express.Router();
let currentPool = null;

router.post("/create", async (req, res) => {
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

router.get("/list", async (req, res) => {
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

router.post("/connect", async (req, res) => {
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

router.post("/delete", async (req, res) => {
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

router.post("/autoCommitOff", async (req, res) => {
  try {
    const client = await pool.connect();
    await client.query("SET AUTOCOMMIT TO OFF");
    res.status(200).json({ message: "Autocommit turned off successfully." });
    client.release();
  } catch (err) {
    console.error("Error setting autocommit:", err);
    res.status(500).json({ message: err.message });
  }
});

export default router;
export { currentPool };