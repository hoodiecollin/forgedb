# ForgeDB Configuration Reference

Complete reference for all ForgeDB configuration options.

## Table of Contents

- [Environment Variables](#environment-variables)
- [Server Configuration](#server-configuration)
- [Database Configuration](#database-configuration)
- [Security Configuration](#security-configuration)
- [Performance Configuration](#performance-configuration)
- [Observability Configuration](#observability-configuration)
- [Example Configurations](#example-configurations)

---

## Environment Variables

All configuration can be set via environment variables with the `FORGEDB_` prefix.

### Server

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `FORGEDB_HOST` | string | `0.0.0.0` | Server bind address |
| `FORGEDB_PORT` | integer | `3000` | Server port |
| `FORGEDB_WORKERS` | integer | CPU count | Number of worker threads |
| `RUST_LOG` | string | `info` | Log level (trace/debug/info/warn/error) |

**Example:**
```bash
export FORGEDB_HOST=127.0.0.1
export FORGEDB_PORT=8080
export RUST_LOG=debug
```

---

### Database

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `FORGEDB_DATA_DIR` | path | `./data` | Database data directory |
| `FORGEDB_SCHEMA_PATH` | path | `./schema.forge` | Path to schema file |
| `FORGEDB_AUTO_COMPACT` | boolean | `true` | Enable automatic compaction |
| `FORGEDB_COMPACT_THRESHOLD` | integer | `10000` | Compaction trigger (dead records) |
| `FORGEDB_WAL_ENABLED` | boolean | `true` | Enable write-ahead log |
| `FORGEDB_WAL_SYNC_INTERVAL_MS` | integer | `1000` | WAL sync interval |

**Example:**
```bash
export FORGEDB_DATA_DIR=/var/lib/forgedb/data
export FORGEDB_SCHEMA_PATH=/etc/forgedb/schema.forge
export FORGEDB_AUTO_COMPACT=true
export FORGEDB_COMPACT_THRESHOLD=5000
```

---

### Security

#### TLS/SSL

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `FORGEDB_TLS_ENABLED` | boolean | `false` | Enable TLS/HTTPS |
| `FORGEDB_TLS_CERT` | path | - | Certificate file path (PEM) |
| `FORGEDB_TLS_KEY` | path | - | Private key file path (PEM) |

**Example:**
```bash
export FORGEDB_TLS_ENABLED=true
export FORGEDB_TLS_CERT=/etc/forgedb/certs/cert.pem
export FORGEDB_TLS_KEY=/etc/forgedb/certs/key.pem
```

#### CORS

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `FORGEDB_CORS_ENABLED` | boolean | `true` | Enable CORS |
| `FORGEDB_CORS_ORIGINS` | string | `*` | Allowed origins (comma-separated) |
| `FORGEDB_CORS_METHODS` | string | `*` | Allowed methods (comma-separated) |
| `FORGEDB_CORS_HEADERS` | string | `*` | Allowed headers (comma-separated) |
| `FORGEDB_CORS_MAX_AGE` | integer | `3600` | Preflight cache duration (seconds) |

**Example:**
```bash
export FORGEDB_CORS_ENABLED=true
export FORGEDB_CORS_ORIGINS=https://app.example.com,https://admin.example.com
export FORGEDB_CORS_METHODS=GET,POST,PUT,DELETE
export FORGEDB_CORS_HEADERS=Content-Type,Authorization
```

#### Authentication

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `FORGEDB_AUTH_ENABLED` | boolean | `false` | Enable authentication |
| `FORGEDB_AUTH_TYPE` | string | `jwt` | Auth type (jwt/apikey/custom) |
| `FORGEDB_AUTH_SECRET` | string | - | Auth secret/key |
| `FORGEDB_AUTH_TOKEN_EXPIRY` | integer | `86400` | Token expiry (seconds) |

**Example:**
```bash
export FORGEDB_AUTH_ENABLED=true
export FORGEDB_AUTH_TYPE=jwt
export FORGEDB_AUTH_SECRET=your-secret-key-here
export FORGEDB_AUTH_TOKEN_EXPIRY=3600
```

---

### Performance

#### Rate Limiting

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `FORGEDB_RATE_LIMIT_ENABLED` | boolean | `true` | Enable rate limiting |
| `FORGEDB_RATE_LIMIT_MAX_REQUESTS` | integer | `100` | Max requests per window |
| `FORGEDB_RATE_LIMIT_WINDOW_SECS` | integer | `60` | Time window (seconds) |

**Example:**
```bash
export FORGEDB_RATE_LIMIT_ENABLED=true
export FORGEDB_RATE_LIMIT_MAX_REQUESTS=500
export FORGEDB_RATE_LIMIT_WINDOW_SECS=60
```

#### Caching

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `FORGEDB_CACHE_ENABLED` | boolean | `true` | Enable response caching |
| `FORGEDB_CACHE_TTL_SECS` | integer | `300` | Default cache TTL (seconds) |
| `FORGEDB_CACHE_MAX_ENTRIES` | integer | `1000` | Maximum cache entries |

**Example:**
```bash
export FORGEDB_CACHE_ENABLED=true
export FORGEDB_CACHE_TTL_SECS=600
export FORGEDB_CACHE_MAX_ENTRIES=5000
```

#### Connection Management

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `FORGEDB_MAX_CONNECTIONS` | integer | `1000` | Max concurrent connections |
| `FORGEDB_REQUEST_TIMEOUT_SECS` | integer | `30` | Request timeout |
| `FORGEDB_KEEP_ALIVE_SECS` | integer | `75` | Keep-alive timeout |

**Example:**
```bash
export FORGEDB_MAX_CONNECTIONS=5000
export FORGEDB_REQUEST_TIMEOUT_SECS=60
export FORGEDB_KEEP_ALIVE_SECS=120
```

---

### Observability

#### Metrics

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `FORGEDB_METRICS_ENABLED` | boolean | `true` | Enable Prometheus metrics |
| `FORGEDB_METRICS_PORT` | integer | `9090` | Metrics endpoint port |
| `FORGEDB_METRICS_PATH` | string | `/metrics` | Metrics endpoint path |

**Example:**
```bash
export FORGEDB_METRICS_ENABLED=true
export FORGEDB_METRICS_PORT=9090
export FORGEDB_METRICS_PATH=/metrics
```

#### Health Checks

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `FORGEDB_HEALTH_ENABLED` | boolean | `true` | Enable health endpoints |
| `FORGEDB_HEALTH_PATH` | string | `/health` | Health check path |

**Example:**
```bash
export FORGEDB_HEALTH_ENABLED=true
export FORGEDB_HEALTH_PATH=/health
```

#### Logging

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `RUST_LOG` | string | `info` | Log level filter |
| `RUST_LOG_FORMAT` | string | `json` | Log format (json/pretty) |
| `FORGEDB_LOG_FILE` | path | - | Log file path (optional) |

**Example:**
```bash
export RUST_LOG=info,forgedb=debug,axum=info
export RUST_LOG_FORMAT=json
export FORGEDB_LOG_FILE=/var/log/forgedb/server.log
```

---

## Example Configurations

### Development

```bash
# Development environment
export FORGEDB_HOST=127.0.0.1
export FORGEDB_PORT=3000
export FORGEDB_DATA_DIR=./data
export RUST_LOG=debug
export FORGEDB_CORS_ENABLED=true
export FORGEDB_CORS_ORIGINS=*
export FORGEDB_RATE_LIMIT_ENABLED=false
export FORGEDB_TLS_ENABLED=false
export FORGEDB_CACHE_ENABLED=false
export RUST_LOG_FORMAT=pretty
```

### Production

```bash
# Production environment
export FORGEDB_HOST=0.0.0.0
export FORGEDB_PORT=3000
export FORGEDB_DATA_DIR=/var/lib/forgedb/data
export RUST_LOG=info
export RUST_LOG_FORMAT=json

# Security
export FORGEDB_TLS_ENABLED=true
export FORGEDB_TLS_CERT=/etc/letsencrypt/live/api.example.com/fullchain.pem
export FORGEDB_TLS_KEY=/etc/letsencrypt/live/api.example.com/privkey.pem
export FORGEDB_CORS_ENABLED=true
export FORGEDB_CORS_ORIGINS=https://app.example.com,https://admin.example.com
export FORGEDB_AUTH_ENABLED=true
export FORGEDB_AUTH_TYPE=jwt
export FORGEDB_AUTH_SECRET=your-secret-key-here

# Performance
export FORGEDB_RATE_LIMIT_ENABLED=true
export FORGEDB_RATE_LIMIT_MAX_REQUESTS=1000
export FORGEDB_RATE_LIMIT_WINDOW_SECS=60
export FORGEDB_CACHE_ENABLED=true
export FORGEDB_CACHE_TTL_SECS=300
export FORGEDB_CACHE_MAX_ENTRIES=10000
export FORGEDB_MAX_CONNECTIONS=5000

# Observability
export FORGEDB_METRICS_ENABLED=true
export FORGEDB_METRICS_PORT=9090
export FORGEDB_HEALTH_ENABLED=true
```

### High Traffic

```bash
# High-traffic environment
export FORGEDB_HOST=0.0.0.0
export FORGEDB_PORT=3000
export FORGEDB_WORKERS=16
export RUST_LOG=warn

# Aggressive rate limiting
export FORGEDB_RATE_LIMIT_MAX_REQUESTS=10000
export FORGEDB_RATE_LIMIT_WINDOW_SECS=60

# Aggressive caching
export FORGEDB_CACHE_ENABLED=true
export FORGEDB_CACHE_TTL_SECS=600
export FORGEDB_CACHE_MAX_ENTRIES=50000

# Connection limits
export FORGEDB_MAX_CONNECTIONS=10000
export FORGEDB_REQUEST_TIMEOUT_SECS=15

# Database optimization
export FORGEDB_AUTO_COMPACT=true
export FORGEDB_COMPACT_THRESHOLD=100000
export FORGEDB_WAL_SYNC_INTERVAL_MS=5000
```

### Docker Compose

```yaml
# docker-compose.yml
version: '3.8'

services:
  forgedb:
    image: forgedb:latest
    ports:
      - "3000:3000"
      - "9090:9090"
    volumes:
      - forgedb-data:/data
      - ./schema.forge:/app/schema.forge:ro
    environment:
      # Server
      FORGEDB_HOST: 0.0.0.0
      FORGEDB_PORT: 3000
      RUST_LOG: info

      # Database
      FORGEDB_DATA_DIR: /data

      # Security
      FORGEDB_CORS_ENABLED: "true"
      FORGEDB_CORS_ORIGINS: https://app.example.com

      # Performance
      FORGEDB_RATE_LIMIT_ENABLED: "true"
      FORGEDB_RATE_LIMIT_MAX_REQUESTS: 500
      FORGEDB_CACHE_ENABLED: "true"

      # Observability
      FORGEDB_METRICS_ENABLED: "true"
      FORGEDB_HEALTH_ENABLED: "true"
    restart: unless-stopped

volumes:
  forgedb-data:
```

---

## Configuration Precedence

Configuration sources are applied in this order (later sources override earlier):

1. Default values (hardcoded in application)
2. Configuration file (if implemented)
3. Environment variables
4. Command-line arguments

---

## Validation

ForgeDB validates configuration on startup and will fail with helpful error messages:

```
Error: Invalid configuration
  - FORGEDB_PORT must be between 1 and 65535
  - FORGEDB_TLS_CERT file not found: /path/to/cert.pem
  - FORGEDB_RATE_LIMIT_MAX_REQUESTS must be positive
```

---

## Best Practices

1. **Use environment-specific configs**: Separate dev, staging, and prod configurations
2. **Secure secrets**: Never commit secrets to version control
3. **Document changes**: Keep a changelog of configuration updates
4. **Test before deploy**: Validate configuration in staging first
5. **Monitor metrics**: Set up alerts for configuration-related issues

---

**See Also:**
- [Deployment Guide](./DEPLOYMENT.md)
- [Troubleshooting Guide](./TROUBLESHOOTING.md)
- [Best Practices](./BEST_PRACTICES.md)
