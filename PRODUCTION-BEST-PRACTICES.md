# Xergon Network - Production Best Practices

**Version:** 1.0.0  
**Last Updated:** 2026-04-12  
**Audience:** DevOps, SRE, Security, Development Teams

---

## 🚀 Deployment Checklist

### Pre-Deployment (Mandatory)

- [ ] **Security Audit Completed**
  - External firm audit report
  - All critical/high findings resolved
  - Re-audit after fixes

- [ ] **Smart Contracts Deployed**
  - Mainnet addresses verified
  - Placeholder addresses replaced
  - Multi-sig treasury configured

- [ ] **Infrastructure Ready**
  - Load balancer configured
  - Auto-scaling policies set
  - Database cluster (PostgreSQL)
  - Redis cache layer

- [ ] **Monitoring & Alerting**
  - Prometheus metrics endpoint
  - Grafana dashboards
  - Alert rules (PagerDuty/Slack)
  - Log aggregation (ELK/Datadog)

- [ ] **Backup & Recovery**
  - Database backup strategy
  - Disaster recovery runbook
  - Regular backup testing

### Deployment Day

- [ ] **Canary Deployment**
  - 5% traffic → monitor 1 hour
  - 25% traffic → monitor 2 hours
  - 50% traffic → monitor 4 hours
  - 100% traffic → full rollout

- [ ] **Smoke Tests**
  - Health checks pass
  - Authentication working
  - Settlement flow functional
  - Provider registration OK

- [ ] **Performance Validation**
  - Load test passed (target: 1000 req/s)
  - Latency p95 < 100ms
  - Error rate < 0.1%

---

## 🔧 Operational Procedures

### Daily Tasks

```bash
# Check system health
curl http://localhost:9090/health
curl http://localhost:9090/metrics | grep xergon_

# Check settlement queue
sqlite3 data/settlement.db "SELECT COUNT(*) FROM pending_settlements;"

# Check provider health
curl http://localhost:9090/providers | jq '.[].health_status'

# Review error logs
tail -100 /var/log/xergon/relay.log | grep ERROR
```

### Weekly Tasks

- [ ] Review Prometheus metrics (error rates, latency trends)
- [ ] Check disk space usage (target: <80% capacity)
- [ ] Rotate API keys (if using time-limited keys)
- [ ] Review security logs for anomalies
- [ ] Backup database and verify integrity

### Monthly Tasks

- [ ] Security patch updates
- [ ] Capacity planning review
- [ ] Disaster recovery test
- [ ] Cost optimization review
- [ ] Team training/upskilling

---

## 🚨 Incident Response

### Severity Levels

| Level | Description | Response Time | Example |
|-------|-------------|---------------|---------|
| **SEV-1** | Complete outage, data loss | <15 minutes | Database corruption, total API failure |
| **SEV-2** | Major functionality broken | <1 hour | Settlement system down, auth failures |
| **SEV-3** | Partial degradation | <4 hours | High latency, occasional errors |
| **SEV-4** | Minor issues | <24 hours | UI bugs, non-critical errors |

### Incident Workflow

1. **Detection** (Automated or Manual)
   - Alert triggered
   - Severity level assigned
   - Incident channel created

2. **Triage** (<5 minutes)
   - On-call engineer acknowledges
   - Initial assessment
   - Escalate if needed

3. **Mitigation** (ASAP)
   - Rollback if recent change
   - Circuit breaker activation
   - Traffic rerouting

4. **Resolution**
   - Root cause analysis
   - Fix implementation
   - Validation testing

5. **Post-Mortem** (<48 hours)
   - Timeline reconstruction
   - Lessons learned
   - Action items created

### Circuit Breaker Recovery

```bash
# Check circuit state
curl http://localhost:9090/circuit-breaker

# If open and issue resolved, manually close
curl -X POST http://localhost:9090/circuit-breaker/close \
  -H "Authorization: Bearer $ADMIN_TOKEN"

# Monitor for 5 minutes before full traffic
```

---

## 📊 Performance Optimization

### Database Tuning (PostgreSQL Migration)

```sql
-- Settlement table optimization
CREATE INDEX idx_settlement_api_key ON settlements(api_key);
CREATE INDEX idx_settlement_status ON settlements(status);
CREATE INDEX idx_settlement_timestamp ON settlements(created_at);

-- Connection pooling
-- Use PgBouncer with 50 connections, 100 max
```

### Caching Strategy

```yaml
# Redis configuration
cache:
  enabled: true
  ttl: 300  # 5 minutes
  max_size: 10000  # entries
  keys:
    - "provider:{id}"  # Provider health
    - "balance:{api_key}"  # User balance
    - "rate_limit:{api_key}"  # Rate limit counter
```

### Load Balancing

```nginx
# Nginx upstream configuration
upstream xergon_relay {
    least_conn;
    server relay-1:9090 weight=3;
    server relay-2:9090 weight=3;
    server relay-3:9090 weight=2;
    keepalive 32;
}

# Rate limiting
limit_req_zone $binary_remote_addr zone=xergon:10m rate=100r/s;
```

---

## 🔐 Security Hardening

### API Security

```yaml
# Required headers
X-API-Key: <api-key>
X-Signature: <HMAC-SHA256>
Content-Type: application/json

# Signature generation (Python example)
import hmac, hashlib, json

def sign_request(api_key, secret, payload):
    message = json.dumps(payload, sort_keys=True)
    signature = hmac.new(
        secret.encode(),
        message.encode(),
        hashlib.sha256
    ).hexdigest()
    return signature
```

### Network Security

- **TLS 1.3:** All external traffic
- **mTLS:** Internal service communication
- **WAF:** Cloudflare/AWS WAF for DDoS protection
- **VPC:** Private subnets for databases

### Key Management

- **API Keys:** HSM-backed (AWS KMS / HashiCorp Vault)
- **Rotation:** 90-day maximum lifetime
- **Storage:** Never in code/repos, use secret manager

### Audit Logging

```json
{
  "timestamp": "2026-04-12T10:30:00Z",
  "event": "api_request",
  "api_key": "xergon-***-1",
  "endpoint": "/v1/chat/completions",
  "status": 200,
  "duration_ms": 45,
  "source_ip": "192.168.1.100"
}
```

---

## 📈 Monitoring & Observability

### Key Metrics

```prometheus
# Request metrics
xergon_requests_total{endpoint, method, status}
xergon_request_duration_seconds{endpoint, quantile}

# Business metrics
xergon_settlements_pending_total
xergon_providers_healthy_gauge
xergon_api_keys_active_total

# System metrics
process_cpu_seconds_total
process_resident_memory_bytes
```

### Alert Rules

```yaml
# Prometheus alerting rules
groups:
  - name: xergon_alerts
    rules:
      - alert: HighErrorRate
        expr: rate(xergon_requests_total{status=~"5.."}[5m]) > 0.05
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "High error rate detected"
          
      - alert: CircuitBreakerOpen
        expr: xergon_circuit_breaker_state == 1
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Circuit breaker is open"
```

### Dashboard Panels

1. **Request Rate & Latency** (p50, p95, p99)
2. **Error Rate by Endpoint**
3. **Provider Health Status**
4. **Settlement Queue Depth**
5. **Circuit Breaker State**
6. **Database Connection Pool**

---

## 🔄 CI/CD Pipeline

### GitHub Actions Example

```yaml
name: CI/CD Pipeline

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Run Tests
        run: cargo test --all
      - name: Security Scan
        run: cargo audit
      - name: Lint
        run: cargo clippy -- -D warnings

  build:
    needs: test
    runs-on: ubuntu-latest
    steps:
      - name: Build Docker
        run: docker build -t xergon-relay:${{ github.sha }} .
      - name: Push to Registry
        run: docker push xergon-relay:${{ github.sha }}

  deploy-staging:
    needs: build
    if: github.ref == 'refs/heads/develop'
    runs-on: ubuntu-latest
    steps:
      - name: Deploy to Staging
        run: kubectl apply -f k8s/staging/

  deploy-production:
    needs: build
    if: github.ref == 'refs/heads/main'
    runs-on: ubuntu-latest
    steps:
      - name: Canary Deployment
        run: kubectl set image deployment/xergon-relay relay=xergon-relay:${{ github.sha }}
      - name: Wait for Health Check
        run: sleep 300 && kubectl rollout status deployment/xergon-relay
```

---

## 💾 Backup & Recovery

### Database Backup Strategy

```bash
# Daily backup script
#!/bin/bash
DATE=$(date +%Y%m%d)
pg_dump xergon_production | gzip > /backups/xergon_${DATE}.sql.gz

# Verify backup
gunzip -c /backups/xergon_${DATE}.sql.gz | head -1

# Retention: 30 days
find /backups -name "xergon_*.sql.gz" -mtime +30 -delete
```

### Disaster Recovery Runbook

**Scenario: Database Corruption**

1. **Assess**
   ```bash
   # Check last backup
   ls -lt /backups/xergon_*.sql.gz | head -1
   
   # Verify integrity
   pg_restore --list /backups/latest.sql.gz
   ```

2. **Restore**
   ```bash
   # Create new database
   createdb xergon_production_new
   
   # Restore backup
   pg_restore -d xergon_production_new /backups/latest.sql.gz
   
   # Swap databases
   kubectl set env deployment/xergon-relay \
     DATABASE_URL=postgres://user:pass@new-db:5432/xergon_production_new
   ```

3. **Validate**
   ```bash
   # Check record counts
   psql xergon_production_new -c "SELECT COUNT(*) FROM settlements;"
   
   # Smoke test API
   curl http://localhost:9090/health
   ```

---

## 🎓 Team Training

### Onboarding Checklist

- [ ] Read `EXPECTATIONS.md` and `PRODUCTION-BEST-PRACTICES.md`
- [ ] Complete security training (OWASP Top 10)
- [ ] Set up development environment
- [ ] Run local tests successfully
- [ ] Deploy to local development
- [ ] Shadow on-call rotation (2 weeks)
- [ ] Lead incident response (supervised)

### Knowledge Transfer

- **Weekly:** Tech talk (30 min)
- **Monthly:** Architecture review
- **Quarterly:** Security training update
- **Annually:** Certification renewal

---

## 📞 Support Contacts

### Internal Team

| Role | Name | Contact |
|------|------|---------|
| **Tech Lead** | [TBD] | tbd@xergon.network |
| **DevOps Lead** | [TBD] | tbd@xergon.network |
| **Security Lead** | [TBD] | security@xergon.network |
| **On-Call** | Rotate | oncall@xergon.network |

### External Services

| Service | Support | SLA |
|---------|---------|-----|
| **Ergo Node** | Community | N/A |
| **Cloud Provider** | AWS Support | 15 min (SEV-1) |
| **Database** | Managed PostgreSQL | 30 min (SEV-1) |

---

## 📚 Reference Documentation

- **Architecture:** `docs/ARCHITECTURE.md`
- **API Reference:** `docs/API_REFERENCE.md`
- **Smart Contracts:** `docs/SMART_CONTRACTS.md`
- **Deployment Guide:** `docs/DEPLOYMENT.md`
- **Troubleshooting:** `docs/TROUBLESHOOTING.md`

---

**Last Reviewed:** 2026-04-12  
**Next Review:** 2026-05-12  
**Owner:** DevOps Team
