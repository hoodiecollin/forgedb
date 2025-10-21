# ForgeDB Deployment Guide

Complete guide for deploying ForgeDB to production environments.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Build for Production](#build-for-production)
- [Deployment Options](#deployment-options)
  - [Docker](#docker)
  - [Systemd (Linux)](#systemd-linux)
  - [Cloud Platforms](#cloud-platforms)
- [Configuration](#configuration)
- [TLS/SSL Setup](#tlsssl-setup)
- [Monitoring](#monitoring)
- [Security](#security)
- [Performance Tuning](#performance-tuning)

---

## Prerequisites

- Rust 1.70+ (for building from source)
- Linux, macOS, or Windows server
- 512MB+ RAM (1GB+ recommended)
- 1GB+ disk space for database and logs

## Build for Production

### 1. Compile Release Build

```bash
# Clone repository
git clone https://github.com/your-org/forgedb.git
cd forgedb

# Build with optimizations
cargo build --release

# Binary will be at: target/release/forgedb
```

### 2. Create Production Schema

```bash
# Create schema.forge file
cat > schema.forge <<'EOF'
User {
  id: +uuid
  email: ^&string @email
  name: &string
  created_at: timestamp
}
EOF

# Generate code
./target/release/forgedb generate
```

---

## Deployment Options

### Docker

#### Dockerfile

```dockerfile
FROM rust:1.70 AS builder

WORKDIR /app
COPY . .

# Build release binary
RUN cargo build --release

FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy binary
COPY --from=builder /app/target/release/forgedb /usr/local/bin/forgedb

# Create data directory
RUN mkdir -p /data

WORKDIR /app

# Copy schema
COPY schema.forge .

# Expose ports
EXPOSE 3000
EXPOSE 9090

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:3000/health || exit 1

# Run server
CMD ["forgedb", "serve", "--host", "0.0.0.0", "--port", "3000"]
```

#### Docker Compose

```yaml
version: '3.8'

services:
  forgedb:
    build: .
    ports:
      - "3000:3000"
      - "9090:9090"  # Metrics
    volumes:
      - ./data:/data
      - ./schema.forge:/app/schema.forge:ro
    environment:
      - RUST_LOG=info
      - FORGEDB_DATA_DIR=/data
      - FORGEDB_CORS_ENABLED=true
      - FORGEDB_RATE_LIMIT_ENABLED=true
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 40s
```

#### Running with Docker

```bash
# Build image
docker build -t forgedb:latest .

# Run container
docker run -d \
  --name forgedb \
  -p 3000:3000 \
  -p 9090:9090 \
  -v $(pwd)/data:/data \
  -v $(pwd)/schema.forge:/app/schema.forge:ro \
  -e RUST_LOG=info \
  forgedb:latest

# View logs
docker logs -f forgedb

# Stop container
docker stop forgedb
```

---

### Systemd (Linux)

#### 1. Create System User

```bash
sudo useradd --system --no-create-home --shell /bin/false forgedb
```

#### 2. Install Binary

```bash
sudo cp target/release/forgedb /usr/local/bin/
sudo chmod +x /usr/local/bin/forgedb
```

#### 3. Create Directory Structure

```bash
sudo mkdir -p /var/lib/forgedb/data
sudo mkdir -p /etc/forgedb
sudo cp schema.forge /etc/forgedb/
sudo chown -R forgedb:forgedb /var/lib/forgedb
sudo chown -R forgedb:forgedb /etc/forgedb
```

#### 4. Create Systemd Service

```bash
sudo nano /etc/systemd/system/forgedb.service
```

```ini
[Unit]
Description=ForgeDB Server
After=network.target

[Service]
Type=simple
User=forgedb
Group=forgedb
WorkingDirectory=/etc/forgedb
ExecStart=/usr/local/bin/forgedb serve --host 0.0.0.0 --port 3000
Restart=on-failure
RestartSec=5s

# Security settings
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/forgedb

# Environment
Environment="RUST_LOG=info"
Environment="FORGEDB_DATA_DIR=/var/lib/forgedb/data"

[Install]
WantedBy=multi-user.target
```

#### 5. Enable and Start Service

```bash
# Reload systemd
sudo systemctl daemon-reload

# Enable service
sudo systemctl enable forgedb

# Start service
sudo systemctl start forgedb

# Check status
sudo systemctl status forgedb

# View logs
sudo journalctl -u forgedb -f
```

---

### Cloud Platforms

#### AWS (EC2 + Application Load Balancer)

1. Launch EC2 instance (t3.micro or larger)
2. Install Rust and build ForgeDB
3. Configure security groups:
   - Allow port 3000 from ALB
   - Allow port 9090 for Prometheus (internal only)
4. Set up ALB with health check on `/health/ready`
5. Configure Auto Scaling based on CPU/memory

#### Google Cloud (Cloud Run)

```bash
# Build container
gcloud builds submit --tag gcr.io/PROJECT_ID/forgedb

# Deploy
gcloud run deploy forgedb \
  --image gcr.io/PROJECT_ID/forgedb \
  --platform managed \
  --region us-central1 \
  --allow-unauthenticated \
  --port 3000 \
  --set-env-vars RUST_LOG=info
```

#### Azure (Container Instances)

```bash
# Create resource group
az group create --name forgedb-rg --location eastus

# Deploy container
az container create \
  --resource-group forgedb-rg \
  --name forgedb \
  --image your-registry.azurecr.io/forgedb:latest \
  --ports 3000 9090 \
  --environment-variables RUST_LOG=info \
  --restart-policy OnFailure
```

---

## Configuration

### Environment Variables

```bash
# Logging
export RUST_LOG=info  # trace, debug, info, warn, error

# Server
export FORGEDB_HOST=0.0.0.0
export FORGEDB_PORT=3000

# Data
export FORGEDB_DATA_DIR=/var/lib/forgedb/data

# CORS
export FORGEDB_CORS_ENABLED=true
export FORGEDB_CORS_ORIGINS=https://example.com,https://app.example.com

# Rate Limiting
export FORGEDB_RATE_LIMIT_ENABLED=true
export FORGEDB_RATE_LIMIT_MAX_REQUESTS=100
export FORGEDB_RATE_LIMIT_WINDOW_SECS=60

# Caching
export FORGEDB_CACHE_ENABLED=true
export FORGEDB_CACHE_TTL_SECS=300
export FORGEDB_CACHE_MAX_ENTRIES=1000

# TLS
export FORGEDB_TLS_ENABLED=true
export FORGEDB_TLS_CERT=/etc/forgedb/certs/cert.pem
export FORGEDB_TLS_KEY=/etc/forgedb/certs/key.pem
```

---

## TLS/SSL Setup

### Option 1: Let's Encrypt (Recommended)

```bash
# Install certbot
sudo apt-get install certbot

# Get certificate
sudo certbot certonly --standalone \
  -d your-domain.com \
  -d api.your-domain.com \
  --email admin@your-domain.com \
  --agree-tos

# Certificates will be at:
# /etc/letsencrypt/live/your-domain.com/fullchain.pem
# /etc/letsencrypt/live/your-domain.com/privkey.pem

# Configure ForgeDB
export FORGEDB_TLS_CERT=/etc/letsencrypt/live/your-domain.com/fullchain.pem
export FORGEDB_TLS_KEY=/etc/letsencrypt/live/your-domain.com/privkey.pem

# Auto-renewal (already set up by certbot)
sudo certbot renew --dry-run
```

### Option 2: Self-Signed (Development Only)

```bash
# Generate certificate
openssl req -x509 -newkey rsa:4096 \
  -keyout key.pem \
  -out cert.pem \
  -days 365 \
  -nodes \
  -subj "/CN=localhost"

# Configure ForgeDB
export FORGEDB_TLS_CERT=./cert.pem
export FORGEDB_TLS_KEY=./key.pem
```

---

## Monitoring

### Prometheus Metrics

ForgeDB exposes Prometheus metrics at `/metrics`:

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'forgedb'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 15s
```

**Available Metrics:**
- `forgedb_http_requests_total` - Total HTTP requests
- `forgedb_http_request_duration_seconds` - Request duration
- `forgedb_db_operations_total` - Database operations
- `forgedb_db_operation_duration_seconds` - DB operation duration
- `forgedb_active_connections` - Active connections
- `forgedb_cache_operations_total` - Cache hits/misses
- `forgedb_errors_total` - Error counts

### Health Checks

- `/health` - Detailed health status
- `/health/live` - Liveness probe (Kubernetes)
- `/health/ready` - Readiness probe (Kubernetes)

### Logging

ForgeDB uses structured logging with tracing:

```bash
# JSON logs for production
export RUST_LOG=info,forgedb=debug

# Pretty logs for development
export RUST_LOG_FORMAT=pretty
```

---

## Security

### 1. Authentication

Implement auth hooks:

```rust
use forgedb_http_server::{AuthHook, AuthContext, JwtAuthHook};

// JWT authentication
let auth_hook = JwtAuthHook::new("your-secret-key".to_string());

// Apply to router
router.layer(middleware::from_fn_with_state(
    Arc::new(auth_hook),
    auth_middleware
))
```

### 2. Rate Limiting

```rust
use forgedb_http_server::{RateLimiter, RateLimitConfig};

let limiter = RateLimiter::new(RateLimitConfig {
    max_requests: 100,
    window_secs: 60,
    enabled: true,
});

router.layer(middleware::from_fn_with_state(
    Arc::new(limiter),
    rate_limit_middleware
))
```

### 3. CORS Configuration

```bash
# Restrict origins in production
export FORGEDB_CORS_ORIGINS=https://app.example.com
export FORGEDB_CORS_METHODS=GET,POST,PUT,DELETE
export FORGEDB_CORS_HEADERS=Content-Type,Authorization
```

### 4. Firewall Rules

```bash
# Allow HTTP/HTTPS only
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp

# Metrics (internal only)
sudo ufw allow from 10.0.0.0/24 to any port 9090

# Enable firewall
sudo ufw enable
```

---

## Performance Tuning

### 1. Connection Limits

```rust
// Limit concurrent connections
use tower::limit::ConcurrencyLimitLayer;

router.layer(ConcurrencyLimitLayer::new(1000))
```

### 2. Request Timeouts

```rust
use tower_http::timeout::TimeoutLayer;
use std::time::Duration;

router.layer(TimeoutLayer::new(Duration::from_secs(30)))
```

### 3. Compression

```rust
use tower_http::compression::CompressionLayer;

router.layer(CompressionLayer::new())
```

### 4. Database Tuning

- Use SSD storage for better IOPS
- Configure appropriate cache size
- Run compaction regularly
- Monitor disk usage

---

## Troubleshooting

See [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) for common issues and solutions.

---

## Production Checklist

- [ ] Build release binary with optimizations
- [ ] Configure TLS with valid certificates
- [ ] Set up authentication/authorization
- [ ] Enable rate limiting
- [ ] Configure CORS properly
- [ ] Set up monitoring (Prometheus + Grafana)
- [ ] Configure health checks
- [ ] Set up log aggregation
- [ ] Configure automated backups
- [ ] Test disaster recovery
- [ ] Document runbooks
- [ ] Set up alerting
- [ ] Load test before launch
- [ ] Configure firewall rules
- [ ] Review security settings

---

**Next Steps:**
- [Configuration Reference](./CONFIGURATION.md)
- [Troubleshooting Guide](./TROUBLESHOOTING.md)
- [Best Practices](./BEST_PRACTICES.md)
