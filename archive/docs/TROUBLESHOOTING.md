# ForgeDB Troubleshooting Guide

Common issues and their solutions.

## Table of Contents

- [Server Issues](#server-issues)
- [Performance Issues](#performance-issues)
- [Security Issues](#security-issues)
- [Database Issues](#database-issues)
- [Monitoring & Logs](#monitoring--logs)

---

## Server Issues

### Server Won't Start

**Symptom:** Server fails to start with error message

**Possible Causes & Solutions:**

1. **Port already in use**
   ```
   Error: Address already in use (os error 98)
   ```
   **Solution:**
   ```bash
   # Find process using port 3000
   sudo lsof -i :3000

   # Kill process
   kill -9 <PID>

   # Or use different port
   export FORGEDB_PORT=3001
   ```

2. **Permission denied (port < 1024)**
   ```
   Error: Permission denied (os error 13)
   ```
   **Solution:**
   ```bash
   # Use port >= 1024
   export FORGEDB_PORT=3000

   # Or grant capability (Linux only)
   sudo setcap CAP_NET_BIND_SERVICE=+eip /path/to/forgedb
   ```

3. **Schema file not found**
   ```
   Error: Schema file not found: schema.forge
   ```
   **Solution:**
   ```bash
   # Check file exists
   ls -la schema.forge

   # Set explicit path
   export FORGEDB_SCHEMA_PATH=/path/to/schema.forge
   ```

### Connection Refused

**Symptom:** Clients cannot connect to server

**Diagnostics:**
```bash
# Check if server is running
ps aux | grep forgedb

# Check if port is listening
netstat -tlnp | grep 3000

# Test connectivity
curl http://localhost:3000/health
```

**Solutions:**

1. **Server not running:** Start the server
2. **Firewall blocking:** Allow port in firewall
   ```bash
   sudo ufw allow 3000/tcp
   ```
3. **Wrong host binding:** Bind to 0.0.0.0 for external access
   ```bash
   export FORGEDB_HOST=0.0.0.0
   ```

### Rate Limit Errors

**Symptom:** 429 Too Many Requests

**Solution:**
```bash
# Increase rate limits
export FORGEDB_RATE_LIMIT_MAX_REQUESTS=1000
export FORGEDB_RATE_LIMIT_WINDOW_SECS=60

# Or disable for specific clients
# Use API key auth to bypass rate limits
```

---

## Performance Issues

### Slow Response Times

**Diagnostics:**
```bash
# Check metrics
curl http://localhost:9090/metrics | grep duration

# Check resource usage
htop

# Enable debug logging
export RUST_LOG=debug
```

**Solutions:**

1. **Enable caching**
   ```bash
   export FORGEDB_CACHE_ENABLED=true
   export FORGEDB_CACHE_TTL_SECS=600
   export FORGEDB_CACHE_MAX_ENTRIES=10000
   ```

2. **Optimize database**
   ```bash
   # Run compaction
   forgedb compact

   # Check database size
   du -sh data/
   ```

3. **Increase worker threads**
   ```bash
   export FORGEDB_WORKERS=16
   ```

4. **Reduce logging**
   ```bash
   export RUST_LOG=warn
   ```

### High Memory Usage

**Diagnostics:**
```bash
# Check memory usage
ps aux | grep forgedb
free -h
```

**Solutions:**

1. **Reduce cache size**
   ```bash
   export FORGEDB_CACHE_MAX_ENTRIES=1000
   ```

2. **Lower connection limit**
   ```bash
   export FORGEDB_MAX_CONNECTIONS=500
   ```

3. **Run garbage collection**
   ```bash
   forgedb compact --aggressive
   ```

### High CPU Usage

**Diagnostics:**
```bash
# Check CPU usage
top -p $(pgrep forgedb)

# Profile with perf (Linux)
perf record -p $(pgrep forgedb) -g -- sleep 10
perf report
```

**Solutions:**

1. **Optimize queries:** Add indexes to frequently queried fields
2. **Enable caching:** Reduce redundant computation
3. **Rate limiting:** Prevent abuse
4. **Load balancing:** Distribute load across multiple instances

---

## Security Issues

### TLS Certificate Errors

**Symptom:** TLS handshake failures

**Diagnostics:**
```bash
# Test TLS connection
openssl s_client -connect localhost:3000 -servername localhost

# Verify certificate
openssl x509 -in cert.pem -text -noout
```

**Solutions:**

1. **Certificate expired**
   ```bash
   # Renew with Let's Encrypt
   sudo certbot renew

   # Restart server
   sudo systemctl restart forgedb
   ```

2. **Certificate not found**
   ```bash
   # Check file exists
   ls -la /etc/forgedb/certs/

   # Set correct paths
   export FORGEDB_TLS_CERT=/path/to/cert.pem
   export FORGEDB_TLS_KEY=/path/to/key.pem
   ```

3. **Certificate/key mismatch**
   ```bash
   # Verify certificate matches key
   openssl x509 -noout -modulus -in cert.pem | openssl md5
   openssl rsa -noout -modulus -in key.pem | openssl md5
   # Hashes should match
   ```

### CORS Errors

**Symptom:** Browser console shows CORS error

**Solution:**
```bash
# Allow specific origins
export FORGEDB_CORS_ORIGINS=https://app.example.com,https://admin.example.com

# Check current CORS settings
curl -I -H "Origin: https://app.example.com" http://localhost:3000/health
```

### Authentication Failures

**Symptom:** 401 Unauthorized

**Diagnostics:**
```bash
# Test with valid token
curl -H "Authorization: Bearer YOUR_TOKEN" http://localhost:3000/api/users

# Check auth configuration
env | grep FORGEDB_AUTH
```

**Solutions:**

1. **Invalid token:** Regenerate token
2. **Expired token:** Increase expiry or refresh token
3. **Wrong auth type:** Check FORGEDB_AUTH_TYPE setting

---

## Database Issues

### Data Loss

**Symptom:** Records missing after restart

**Possible Causes:**

1. **WAL not synced**
   ```bash
   # Enable WAL with frequent syncing
   export FORGEDB_WAL_ENABLED=true
   export FORGEDB_WAL_SYNC_INTERVAL_MS=1000
   ```

2. **Corruption:** Check logs for errors
   ```bash
   grep -i error /var/log/forgedb/server.log
   ```

**Recovery:**
```bash
# Restore from backup
cp -r /backups/forgedb/data-2024-01-15/ /var/lib/forgedb/data/

# Rebuild indexes
forgedb rebuild-indexes
```

### Database Corruption

**Symptom:** Errors reading/writing data

**Diagnostics:**
```bash
# Check database integrity
forgedb check-integrity

# View logs
tail -f /var/log/forgedb/server.log
```

**Solutions:**

1. **Restore from backup**
   ```bash
   # Stop server
   sudo systemctl stop forgedb

   # Restore data
   rm -rf /var/lib/forgedb/data
   cp -r /backups/latest/ /var/lib/forgedb/data/

   # Start server
   sudo systemctl start forgedb
   ```

2. **Rebuild database**
   ```bash
   # Export data
   forgedb export --output backup.json

   # Clear database
   rm -rf data/

   # Import data
   forgedb import --input backup.json
   ```

### Slow Queries

**Diagnostics:**
```bash
# Enable query logging
export RUST_LOG=debug,forgedb::storage=trace

# Check for missing indexes
grep "full scan" /var/log/forgedb/server.log
```

**Solutions:**

1. **Add indexes** to frequently queried fields:
   ```forge
   User {
     email: ^&string @email  // ^ adds unique index
     created_at: timestamp @index(created_at)
   }
   ```

2. **Run compaction:**
   ```bash
   forgedb compact
   ```

---

## Monitoring & Logs

### No Metrics

**Symptom:** /metrics endpoint returns no data

**Solution:**
```bash
# Enable metrics
export FORGEDB_METRICS_ENABLED=true
export FORGEDB_METRICS_PORT=9090

# Test endpoint
curl http://localhost:9090/metrics
```

### Logs Not Appearing

**Diagnostics:**
```bash
# Check log level
echo $RUST_LOG

# Check systemd logs
sudo journalctl -u forgedb -f

# Check log file
tail -f /var/log/forgedb/server.log
```

**Solutions:**

1. **Increase log level:**
   ```bash
   export RUST_LOG=debug
   ```

2. **Enable stdout logging:**
   ```bash
   export RUST_LOG_FORMAT=pretty
   ```

3. **Check file permissions:**
   ```bash
   ls -la /var/log/forgedb/
   chown forgedb:forgedb /var/log/forgedb/
   ```

### Health Check Failing

**Symptom:** /health endpoint returns 503

**Diagnostics:**
```bash
# Check detailed health
curl http://localhost:3000/health | jq

# Check readiness
curl http://localhost:3000/health/ready
```

**Solutions:**

1. **Database not ready:** Wait for startup
2. **Dependency failure:** Check logs for errors
3. **Resource exhaustion:** Check CPU/memory

---

## Common Error Messages

### "Address already in use"
**Cause:** Port conflict
**Solution:** Change port or kill conflicting process

### "Permission denied"
**Cause:** Insufficient permissions
**Solution:** Run as sudo or adjust permissions

### "Connection refused"
**Cause:** Server not running or firewall
**Solution:** Start server or configure firewall

### "Schema validation failed"
**Cause:** Invalid schema syntax
**Solution:** Check schema file for errors

### "Rate limit exceeded"
**Cause:** Too many requests
**Solution:** Increase rate limits or implement backoff

---

## Getting Help

If you still have issues:

1. **Check logs:** Enable debug logging (`RUST_LOG=debug`)
2. **Search issues:** https://github.com/your-org/forgedb/issues
3. **Ask community:** https://discord.gg/forgedb
4. **File bug report:** Include logs, config, and steps to reproduce

---

**See Also:**
- [Deployment Guide](./DEPLOYMENT.md)
- [Configuration Reference](./CONFIGURATION.md)
- [Best Practices](./BEST_PRACTICES.md)
