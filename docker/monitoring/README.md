# Xergon Network -- Production Monitoring

## Quick Start

```bash
# Start the monitoring stack
docker compose -f docker/monitoring/docker-compose.yml up -d

# Access dashboards
# Prometheus: http://localhost:9090
# Grafana:    http://localhost:3001 (admin/admin)
# Alertmanager: http://localhost:9093
```

## Components

| Service | Port | Purpose |
|---------|------|---------|
| Prometheus | 9090 | Metrics collection (15s scrape interval) |
| Grafana | 3001 | Dashboards and visualization |
| Alertmanager | 9093 | Alert routing and deduplication |

## Metrics Endpoints

- **xergon-agent**: `http://<agent-ip>:9099/api/metrics` (Prometheus format)
- **xergon-relay**: `http://<relay-ip>:8080/v1/metrics` (Prometheus format)

## Configuring Targets

Edit `docker/monitoring/prometheus.yml` to point to your actual agent/relay IPs:

```yaml
static_configs:
  - targets:
    - 'your-agent-ip:9099'
    - 'your-relay-ip:8080'
```

For multiple providers, add additional target entries.

## Alert Configuration

### Critical Alerts (immediate)
- **XergonAgentDown** -- Agent unreachable for 2+ minutes
- **XergonRelayDown** -- Relay unreachable for 2+ minutes
- **HighInferenceErrorRate** -- Error rate > 10% over 5 minutes

### Warning Alerts (investigate within 1 hour)
- **HighInferenceLatency** -- Avg latency > 30s
- **NoActiveProviders** -- Relay has 0 active providers
- **LowWalletBalance** -- Agent wallet < 0.5 ERG
- **HighRelay5xxRate** -- Relay 5xx rate > 5%
- **ChainSyncLag** -- Node behind by > 10 blocks

### Notification Channels

Edit `docker/monitoring/alertmanager.yml` to configure your webhook URLs:
- `default-webhook` -- All alerts
- `critical-webhook` -- Critical alerts (immediate, 1h repeat)
- `warning-webhook` -- Warning alerts (4h repeat)

## Dashboards

Pre-configured Grafana dashboard at http://localhost:3001:
- Overview: uptime, request rate, latency, error rate, wallet balance, active providers
- Time series: inference rate, latency, tokens processed, chain sync, P2P peers
- Relay: request breakdown by type, 4xx/5xx errors

## Retention

Prometheus stores 30 days of data by default. Adjust in `docker-compose.yml`:
```yaml
command:
  - '--storage.tsdb.retention.time=30d'
```
