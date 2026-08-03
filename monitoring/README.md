# Vimbai Monitoring Stack

## Overview

This directory contains Docker configurations for the Vimbai monitoring stack using Prometheus and Grafana.

## Components

- **Prometheus**: Metrics collection and storage
- **Grafana**: Visualization and dashboards
- **Alertmanager**: Alert routing (optional)

## Quick Start

```bash
# Start monitoring stack
docker-compose -f docker-compose.monitoring.yml up -d

# Access Grafana
# URL: http://localhost:3000
# Default credentials: admin / admin
```

## Services

### Prometheus (Port 9090)
- Scrapes metrics from all Vimbai services
- Stores time-series data
- Runs alert rules

### Grafana (Port 3000)
- Dashboards for service monitoring
- Alert visualization
- Custom queries

### Alertmanager (Port 9093)
- Alert routing and deduplication
- Notification channels (email, Slack, PagerDuty)

## Dashboards

### Service Overview
- Request rate per service
- Error rate monitoring
- Latency percentiles (p50, p95, p99)

### Database Performance
- Neo4j query times
- Connection pool usage
- Transaction throughput

### Business Metrics
- Journal entries created
- Accounts managed
- NPO donations/grants

## Alerting Rules

Critical alerts:
- High error rate (>5%)
- Slow responses (>2s p95)
- Database connection exhaustion
- Service down

Warning alerts:
- Elevated error rate (>1%)
- Latency increase (>50%)
- Disk usage >80%

## Maintenance

```bash
# Check Prometheus targets
curl http://localhost:9090/api/v1/targets

# View Grafana logs
docker-compose -f docker-compose.monitoring.yml logs grafana

# Reset Grafana password
docker exec -it grafana grafana-cli admin reset-admin-password newpassword
```