// the function to be imported for the pg-admin
import pgp from 'pg-promise' // Import pg-promise

var types = require('pg').types

// Initializing the library:

types.setTypeParser(1082, val => val)

const initOptions = {
  noWarnings: true
}

// Database connection details:
const cn = {
  user: process.env.DB_USER,
  host: process.env.DB_HOST,
  password: process.env.DB_PASSWORD,
  port: process.env.DB_PORT
}

const db = pgp(initOptions)(cn)

export default db

// Query Function:
export const query = async (text, params) => {
  try {
    const result = await db.any(text, params) // Using .any as an example. You might want to use .one, .none, etc. based on your needs.

    return result
  } catch (error) {
    console.error('Error executing query:', error)
    throw error
  }
}

// Transaction Functions:
export const beginTransaction = async () => {
  return await query('BEGIN')
}

export const commitTransaction = async () => {
  return await query('COMMIT')
}

export const rollbackTransaction = async () => {
  return await query('ROLLBACK')
}

// If you need to end the connection pool (not commonly done in web apps as the pool should stay up):
export const endConnection = () => {
  db.$pool.end()
}
