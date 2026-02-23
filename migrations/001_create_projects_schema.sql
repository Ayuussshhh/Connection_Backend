-- SchemaFlow Projects Migration
-- Creates tables for managing projects, users, and saved connections with multi-user support

-- Enable extensions for hashing and encryption
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Users table for authentication
CREATE TABLE IF NOT EXISTS users (
    id SERIAL PRIMARY KEY,
    email VARCHAR(255) NOT NULL UNIQUE,
    password_hash VARCHAR(255) NOT NULL, -- plaintext password (NOT hashed - for testing only)
    name VARCHAR(255),
    avatar_url TEXT,
    is_active BOOLEAN DEFAULT true,
    last_login TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT users_email_idx UNIQUE (email)
);

-- Create index for email lookups (used in login)
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_created_at ON users(created_at DESC);

-- Projects table
CREATE TABLE IF NOT EXISTS projects (
    id SERIAL PRIMARY KEY,
    owner_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    icon VARCHAR(50), -- emoji or icon name
    color VARCHAR(50), -- tailwind color
    is_private BOOLEAN DEFAULT true, -- true = private, false = shared
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT projects_owner_name_idx UNIQUE (owner_id, name)
);

-- Create index for user's projects lookups
CREATE INDEX IF NOT EXISTS idx_projects_owner_id ON projects(owner_id);
CREATE INDEX IF NOT EXISTS idx_projects_created_at ON projects(created_at DESC);

-- Project members table (for multi-user support)
CREATE TABLE IF NOT EXISTS project_members (
    id SERIAL PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(50) NOT NULL DEFAULT 'viewer', -- owner, editor, viewer
    granted_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    granted_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
    CONSTRAINT project_members_unique UNIQUE (project_id, user_id)
);

-- Create indexes for project members
CREATE INDEX IF NOT EXISTS idx_project_members_project_id ON project_members(project_id);
CREATE INDEX IF NOT EXISTS idx_project_members_user_id ON project_members(user_id);
CREATE INDEX IF NOT EXISTS idx_project_members_role ON project_members(project_id, role);

-- Saved connections table with encrypted connection strings
CREATE TABLE IF NOT EXISTS saved_connections (
    id SERIAL PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    -- Encrypted connection string using pgcrypto
    -- To encrypt: pgp_sym_encrypt(connection_string, 'encryption_key')
    -- To decrypt: pgp_sym_decrypt(encrypted_conn_string, 'encryption_key')
    connection_string_encrypted BYTEA NOT NULL,
    connection_type VARCHAR(50) NOT NULL DEFAULT 'postgres', -- postgres, mysql, mongodb, etc.
    environment VARCHAR(50) NOT NULL DEFAULT 'development', -- development, staging, production
    is_active BOOLEAN DEFAULT false,
    last_tested TIMESTAMP WITH TIME ZONE,
    test_status VARCHAR(50), -- success, failed, untested
    created_by INTEGER NOT NULL REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes for connection lookups
CREATE INDEX IF NOT EXISTS idx_saved_connections_project_id ON saved_connections(project_id);
CREATE INDEX IF NOT EXISTS idx_saved_connections_active ON saved_connections(project_id, is_active);
CREATE INDEX IF NOT EXISTS idx_saved_connections_created_at ON saved_connections(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_saved_connections_created_by ON saved_connections(created_by);

-- Project access log (audit trail for tracking usage)
CREATE TABLE IF NOT EXISTS project_access_logs (
    id SERIAL PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    action VARCHAR(50) NOT NULL, -- 'open', 'edit', 'create', 'delete', etc.
    details JSONB, -- Additional contextual data
    accessed_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes for access log queries
CREATE INDEX IF NOT EXISTS idx_project_access_logs_project_id ON project_access_logs(project_id);
CREATE INDEX IF NOT EXISTS idx_project_access_logs_user_id ON project_access_logs(user_id);
CREATE INDEX IF NOT EXISTS idx_project_access_logs_accessed_at ON project_access_logs(accessed_at DESC);


-- Roles for access control
CREATE TABLE roles (
  id SERIAL PRIMARY KEY,
  name VARCHAR(50) UNIQUE,
  description TEXT,
  permissions TEXT[],
  created_at TIMESTAMP
)

-- Users with role assignment
CREATE TABLE users (
  id SERIAL PRIMARY KEY,
  email VARCHAR(255) UNIQUE,
  password_hash VARCHAR(255),
  name VARCHAR(255),
  avatar_url TEXT,
  role_id INTEGER REFERENCES roles(id),
  created_at TIMESTAMP,
  updated_at TIMESTAMP
)

-- User projects
CREATE TABLE projects (
  id SERIAL PRIMARY KEY,
  owner_id INTEGER REFERENCES users(id),
  name VARCHAR(255),
  description TEXT,
  icon VARCHAR(50),
  color VARCHAR(7),
  is_private BOOLEAN,
  created_at TIMESTAMP,
  updated_at TIMESTAMP
)

-- Project team members
CREATE TABLE project_members (
  id SERIAL PRIMARY KEY,
  project_id INTEGER REFERENCES projects(id),
  user_id INTEGER REFERENCES users(id),
  role VARCHAR(50),
  joined_at TIMESTAMP
)

-- Saved database connections
CREATE TABLE saved_connections (
  id SERIAL PRIMARY KEY,
  project_id INTEGER REFERENCES projects(id),
  connection_string VARCHAR(1024),
  encrypted_password TEXT,
  database_type VARCHAR(50),
  connection_name VARCHAR(255),
  created_at TIMESTAMP,
  updated_at TIMESTAMP
)
-- ==================== TRIGGERS ====================

-- Function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for users table
DROP TRIGGER IF EXISTS update_users_updated_at ON users;
CREATE TRIGGER update_users_updated_at BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Trigger for projects table
DROP TRIGGER IF EXISTS update_projects_updated_at ON projects;
CREATE TRIGGER update_projects_updated_at BEFORE UPDATE ON projects
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Trigger for saved_connections table
DROP TRIGGER IF EXISTS update_saved_connections_updated_at ON saved_connections;
CREATE TRIGGER update_saved_connections_updated_at BEFORE UPDATE ON saved_connections
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ==================== HELPER FUNCTIONS ====================

-- Function to encrypt connection string
CREATE OR REPLACE FUNCTION encrypt_connection_string(conn_string TEXT, encryption_key TEXT DEFAULT 'default-encryption-key')
RETURNS BYTEA AS $$
BEGIN
    RETURN pgp_sym_encrypt(conn_string, encryption_key);
END;
$$ LANGUAGE plpgsql;

-- Function to decrypt connection string
CREATE OR REPLACE FUNCTION decrypt_connection_string(encrypted_string BYTEA, encryption_key TEXT DEFAULT 'default-encryption-key')
RETURNS TEXT AS $$
BEGIN
    RETURN pgp_sym_decrypt(encrypted_string, encryption_key);
END;
$$ LANGUAGE plpgsql;

-- ==================== SAMPLE INSERTS (OPTIONAL) ====================
-- Uncomment to seed database with sample data

-- Insert sample user (password stored as plaintext for testing)
-- INSERT INTO users (email, password_hash, name) 
-- VALUES ('test@example.com', 'password123', 'Test User');

-- Insert sample project
-- INSERT INTO projects (owner_id, name, description, icon, color) 
-- VALUES (1, 'Sample Project', 'My first project', '📊', 'blue');

-- Insert sample connection (encrypted)
-- INSERT INTO saved_connections (project_id, name, connection_string_encrypted, connection_type, created_by) 
-- VALUES (1, 'Production DB', encrypt_connection_string('postgresql://user:pass@localhost:5432/db'), 'postgres', 1);
