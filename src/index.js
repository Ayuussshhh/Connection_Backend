import express from "express";
import dotenv from "dotenv";
import databaseRoutes from "./routes/databaseRoute.js";
import tableRoutes from "./routes/tableRoutes.js";
import pool from "./utils/database.js";

dotenv.config();
const app = express();
const PORT = process.env.PORT || 3000;

app.use(express.json());

// Routes
app.use("/db", databaseRoutes);
app.use("/table", tableRoutes);

app.get("/health", (req, res) => {
  res.status(200).json({ message: "Server is running fine." });
});

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