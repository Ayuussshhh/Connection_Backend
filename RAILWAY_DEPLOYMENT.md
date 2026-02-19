# Railway Deployment Guide for SchemaFlow API

This guide will help you deploy the SchemaFlow Rust backend to Railway.app using GitHub.

## Prerequisites

- Railway account ([railway.app](https://railway.app))
- GitHub account with your repository pushed
- Your repository should have the `Backend/` directory with all the Rust code

## Step-by-Step Deployment Instructions

### 1. **Prepare Your Repository**

Make sure all the Railway configuration files are committed to your GitHub repository:

```bash
git add Backend/Procfile Backend/railway.json Backend/nixpacks.toml Backend/src/config.rs
git commit -m "Add Railway deployment configuration"
git push origin main
```

### 2. **Create a New Railway Project**

1. Go to [railway.app](https://railway.app) and log in
2. Click **"New Project"**
3. Select **"Deploy from GitHub repo"**
4. Choose your repository: `Ayuussshhh/Connection_Backend`
5. Railway will start the deployment automatically

### 3. **Configure the Root Directory**

Since your Rust code is in the `Backend/` subdirectory:

1. In your Railway project dashboard, click on your service
2. Go to **Settings** tab
3. Find **"Root Directory"** or **"Service Settings"**
4. Set the root directory to: `Backend`
5. Click **"Save"**

### 4. **Set Environment Variables**

In the Railway dashboard:

1. Click on your service
2. Go to the **Variables** tab
3. Add the following environment variables:

#### Required Variables:

```bash
# JWT Secret (generate a secure random string)
JWT_SECRET=your-super-secret-jwt-key-change-this-to-something-random

# Server Configuration
HOST=0.0.0.0
PORT=${{PORT}}  # Railway automatically sets this

# CORS - Add your frontend URL
ALLOWED_ORIGINS=https://your-frontend-domain.com,http://localhost:3001

# Logging
RUST_LOG=info,schemaflow_api=debug
```

#### Optional Variables (if using a PostgreSQL database):

If you want to add a PostgreSQL database to your Railway project:

1. Click **"New"** > **"Database"** > **"Add PostgreSQL"**
2. Railway will automatically create `DATABASE_URL` variable
3. Add these additional variables:

```bash
DB_HOST=${{POSTGRES.PGHOST}}
DB_PORT=${{POSTGRES.PGPORT}}
DB_USER=${{POSTGRES.PGUSER}}
DB_PASSWORD=${{POSTGRES.PGPASSWORD}}
DB_NAME=${{POSTGRES.PGDATABASE}}
DB_MAX_POOL_SIZE=10
```

### 5. **Deploy**

Once you've saved all environment variables:

1. Railway will automatically trigger a new deployment
2. Or manually trigger by clicking **"Deploy"** in the **Deployments** tab
3. Watch the build logs to ensure everything compiles correctly

The build process will:
- Install Rust toolchain
- Install OpenSSL and pkg-config dependencies
- Run `cargo build --release`
- Start the server with `./target/release/schemaflow-api`

### 6. **Get Your Deployment URL**

1. In the Railway dashboard, go to **Settings**
2. Under **"Domains"**, click **"Generate Domain"**
3. Railway will provide you with a public URL like: `your-app.railway.app`
4. Your API will be accessible at: `https://your-app.railway.app`

### 7. **Test Your Deployment**

Test the health of your deployment:

```bash
curl https://your-app.railway.app/api/auth/login
```

You should get a response (even if it's an error for missing credentials, it means the server is running).

## Common Issues & Solutions

### Issue 1: Build Fails with "edition2024" Error

**Solution:** This has been fixed by updating Rust to 1.93.1. Railway uses the latest stable Rust.

### Issue 2: Server Not Accessible

**Problem:** Server binds to `127.0.0.1` instead of `0.0.0.0`

**Solution:** Make sure `HOST=0.0.0.0` is set in environment variables.

### Issue 3: Port Binding Error

**Problem:** Server tries to bind to port 3000 but Railway assigns a different port

**Solution:** Railway automatically sets the `PORT` environment variable. The code already handles this in `config.rs`.

### Issue 4: Build Takes Too Long / Times Out

**Solution:** 
- Railway has build time limits on the free tier
- The first build will be slow (~5-10 minutes) due to compiling dependencies
- Subsequent builds use caching and are much faster

### Issue 5: Binary Not Found

**Problem:** Railway can't find the executable

**Solution:** Check that the binary name in `Procfile` matches your `Cargo.toml`:
- In `Cargo.toml`: `name = "schemaflow-api"`
- In `Procfile`: `web: ./target/release/schemaflow-api`

### Issue 6: OpenSSL Errors

**Solution:** The `nixpacks.toml` file includes OpenSSL and pkg-config. If you still get errors, verify the file exists.

## Monitoring & Logs

### View Logs

1. Go to your Railway project
2. Click on your service
3. Click on **"Deployments"** tab
4. Click on the active deployment
5. View real-time logs

### Common Log Commands

Filter logs:
```bash
# In Railway dashboard, use the search feature to filter logs
```

## Updating Your Deployment

Whenever you push to your GitHub repository:

```bash
git add .
git commit -m "Your changes"
git push origin main
```

Railway will automatically detect the changes and redeploy your application.

## Rolling Back

If a deployment fails:

1. Go to **Deployments** tab
2. Find a previous successful deployment
3. Click the **three dots** menu
4. Select **"Redeploy"**

## Production Checklist

Before going to production:

- [ ] Set a strong `JWT_SECRET` (use a password generator)
- [ ] Configure proper `ALLOWED_ORIGINS` for your frontend
- [ ] Add a PostgreSQL database if needed
- [ ] Set `RUST_LOG=info` (not `debug`) for production
- [ ] Enable Railway's built-in metrics
- [ ] Set up health checks
- [ ] Configure custom domain (optional)
- [ ] Review Railway's pricing and set spending limits

## Cost Optimization

- Railway's free tier includes $5 of usage per month
- Rust applications are very memory-efficient
- Use `cargo build --release` (already configured) for optimized builds
- Consider using Railway's "sleep on idle" feature for development environments

## Support & Resources

- [Railway Documentation](https://docs.railway.app/)
- [Railway Discord](https://discord.gg/railway)
- [Railway Status Page](https://status.railway.app/)
- [SchemaFlow API Documentation](./README.md)

## Architecture Notes

The SchemaFlow API supports **dynamic database connections**. Users can connect to any PostgreSQL database through the API:

```bash
POST /api/connections
{
  "connection_string": "postgresql://user:pass@host:5432/dbname"
}
```

This means you don't need to configure a database in Railway environment variables unless you want to use the legacy endpoints.

## Next Steps

After successful deployment:

1. Test all API endpoints
2. Connect your frontend to the Railway URL
3. Create a default admin user (automatically created on first run)
4. Test database connections through the API
5. Monitor logs and metrics

---

**Need Help?** Check the Railway documentation or open an issue in the GitHub repository.
