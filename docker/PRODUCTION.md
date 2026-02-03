<!--
Copyright (c) 2025-present Cesar Saguier Antebi

This file is part of AIGEN Blockchain.

This source code is licensed under the Business Source License 1.1
found in the LICENSE file in the root directory of this source tree.
-->

# Production Deployment Guide

## Cloud Deployment Considerations

### AWS ECS/EKS

- Use ECR for Docker image registry
- ECS task definitions for each node type
- Application Load Balancer for RPC endpoints
- EFS for persistent blockchain data
- CloudWatch for logging and monitoring

### DigitalOcean

- Droplet per node (2 vCPU, 4GB RAM minimum)
- Block storage volumes for data persistence
- Floating IPs for stable addressing
- Managed Kubernetes for orchestration

## Security Hardening

- Run containers as non-root user (already configured)
- Enable read-only root filesystem where possible
- Use secrets management for CEO keys (never in containers)
- Network policies to restrict inter-node communication
- TLS/SSL for RPC endpoints (reverse proxy with nginx/traefik)
- Rate limiting on public RPC endpoints
- Firewall rules: Allow 9000/tcp (P2P) only from known peers, 9944/tcp (RPC) only from trusted IPs

## Monitoring and Alerting

- Prometheus metrics export (requires implementation)
- Grafana dashboards for visualization
- Alert on: peer count drop, sync stall, high memory usage, shutdown signal
- Log aggregation with ELK stack or Loki

## Backup Strategy

- Regular snapshots of blockchain data volumes
- Backup node keypairs securely (encrypted, offline storage)
- CEO key management: HSM or hardware wallet, never on node servers
- Disaster recovery plan: Restore from snapshot, resync from peers

## Performance Tuning

- Increase `max_peers` for better connectivity
- Tune gossipsub parameters for network size
- Adjust RPC `max_connections` based on load
- Use SSD storage for blockchain data
- Enable CPU pinning for consensus-critical nodes
