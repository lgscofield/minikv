# Professional Test Report - Example

Scenario: Node Failure (Manual Validation)
Template version: minikv v1.0.0

## General Information

- Date: 2026-04-08
- Tester: Em
- Scenario ID and Name: 6 - Node Failure
- Build/Version (`git rev-parse --short HEAD`): local working tree
- Environment (local, Docker, k8s): Docker Compose (local)
- Cluster configuration (coordinator, volumes, replicas): 1 coordinator, 3 volumes, replication factor 3

## Objective

- Objective: Validate cluster availability and recovery when one volume becomes unavailable.
- Scope: Health endpoints, read/write behavior during outage, and post-restart recovery.
- Preconditions:
  - Cluster started and healthy.
  - Initial dataset written successfully.
  - Metrics endpoint reachable.

## Steps Executed

1. Started cluster with coordinator + 3 volumes.
2. Inserted baseline dataset (1000 keys).
3. Stopped one volume container (`minikv-volume-1`).
4. Checked `GET /health/live`, `GET /health/ready`, and `GET /metrics`.
5. Executed read/write requests while one volume was down.
6. Restarted stopped volume.
7. Re-checked health, metrics, and data consistency.

## Commands Used

```bash
# Start stack
docker compose up -d

# Baseline writes (example)
for i in $(seq 1 1000); do
  curl -s -X PUT "http://localhost:8080/s3/test-bucket/key-$i" \
    --data "value-$i" >/dev/null
 done

# Stop one volume
docker stop minikv-volume-1

# Health and metrics checks
curl -s http://localhost:8080/health/live
curl -s http://localhost:8080/health/ready
curl -s http://localhost:8080/metrics | head -n 40

# Sample reads during outage
curl -i -s "http://localhost:8080/s3/test-bucket/key-10"
curl -i -s "http://localhost:8080/s3/test-bucket/key-500"

# Restart failed volume
docker start minikv-volume-1

# Post-recovery checks
curl -s http://localhost:8080/health/live
curl -s http://localhost:8080/health/ready
curl -s http://localhost:8080/metrics | head -n 40
```

## Verification Points

- Health endpoints remain responsive during single-volume outage.
- Read/write operations continue with degraded capacity.
- Restarted volume rejoins cluster.
- Data consistency remains acceptable after recovery.

## Observed Results

- Coordinator remained reachable throughout the outage window.
- Read operations on unaffected paths continued successfully.
- Write operations completed with expected degraded behavior.
- After restart, the stopped volume returned to healthy participation.

## Metrics and Logs

- Metrics endpoint sample (`GET /metrics`): reachable before, during, and after outage.
- Health endpoints (`GET /health/live`, `GET /health/ready`): returned success status.
- Coordinator logs: showed volume down event and subsequent recovery.
- Volume logs: restarted volume rejoined and resumed normal processing.
- Additional evidence: container lifecycle events from Docker.

## Scenario Status

- [x] Pass
- [ ] Fail
- Notes: Single-volume failure tolerance behaves as expected for this topology.

## Feature-Specific Checklist

### Kubernetes Operator and Cloud-Native

- [ ] CRD applied and recognized
- [ ] Operator reconciliation successful
- [ ] StatefulSet, Services, ConfigMaps, RBAC validated
- [ ] Scaling behavior verified

### Time-Series

- [ ] `POST /ts/write` validated
- [ ] `POST /ts/query` validated
- [ ] Aggregation/filter behavior verified

### Vector Search

- [ ] `POST /vector/upsert` validated
- [ ] `POST /vector/query` validated
- [ ] `GET /admin/vector/stats` validated
- [ ] Persistence across restart verified

### Geo-Partitioning

- [ ] Routing strategies validated
- [ ] Failover validated
- [ ] Geo-fencing validated (if configured)

### Data Tiering

- [ ] Tier transitions validated
- [ ] Readability after movement validated

### io_uring (Linux)

- [ ] io_uring path active when enabled
- [ ] Fallback path validated

### Reliability and Consistency

- [x] Node failure recovery validated
- [ ] Split-brain resistance validated
- [ ] Consistency across replicas validated

### Operations and Security

- [ ] Compaction and repair safety validated
- [ ] Audit logging validated
- [ ] Persistent backend restart durability validated

### Watch and Subscribe

- [ ] `/watch/ws` validated
- [ ] `/watch/sse` validated
- [ ] Event payload and ordering validated

## Attachments

- Screenshots: N/A
- Log extracts: pending archive
- Metrics snapshots: pending archive
- Additional artifacts: Docker event timeline

## Final Notes

- Risks identified: Multi-volume concurrent failure not covered by this run.
- Follow-up actions: Execute split-brain and consistency scenarios in the same environment.
- Owner: Em
- Target date: 2026-04-10
