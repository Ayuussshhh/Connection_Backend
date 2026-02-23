@echo off
echo.
echo ================================================================
echo   SchemaFlow Database Migration
echo ================================================================
echo.

REM Connection string for Neon
set CONN=postgresql://neondb_owner:npg_2MkU1GXnLvdg@ep-ancient-surf-ai5twp8j-pooler.c-4.us-east-1.aws.neon.tech/neondb?sslmode=require^&channel_binding=require

REM Migration file
set MIGRATION_FILE=%~dp0001_create_projects_schema.sql

echo Migration file: %MIGRATION_FILE%
echo.

REM Check if psql exists
where psql >nul 2>nul
if %errorlevel% equ 0 (
    echo Found psql. Running migration...
    echo.
    psql "%CONN%" -f "%MIGRATION_FILE%"
    if %errorlevel% equ 0 (
        echo.
        echo Migration completed successfully!
        echo.
    ) else (
        echo.
        echo Migration failed with error code %errorlevel%
        pause
    )
) else (
    echo PostgreSQL psql not found in PATH.
    echo.
    echo To run the migration, use ONE of these options:
    echo.
    echo Option 1: Install PostgreSQL
    echo   https://www.postgresql.org/download/windows/
    echo.
    echo Option 2: Use Neon Web Console (Recommended)
    echo   1. Go to https://console.neon.tech
    echo   2. Open SQL Editor
    echo   3. Copy contents of: %MIGRATION_FILE%
    echo   4. Paste and click Run
    echo.
    echo Option 3: Use DBeaver or pgAdmin
    echo   Connection: %CONN%
    echo.
    pause
)
