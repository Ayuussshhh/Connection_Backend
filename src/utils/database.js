import {Pool} from 'pg';
import dotenv from "dotenv";
dotenv.config();

const pool = new Pool({
    user: process.env.DB_USER,
    host: process.env.DB_HOST,
    port: process.env.DB_PORT,
    password: process.env.DB_PASSWORD
})

export default pool;