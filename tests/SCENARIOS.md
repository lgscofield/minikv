# Professional Test Scenarios - minikv v1.0.0

This document defines manual validation scenarios for minikv v1.0.0.
Each scenario includes context, steps, and success criteria.

## 0. Kubernetes Operator and Cloud-Native Deployment

Context: Validate CRD lifecycle, reconciliation, RBAC, StatefulSet behavior, and scaling.

Steps:
1. Deploy CRD and operator manifests.
2. Apply basic and production cluster examples.
3. Verify StatefulSet, Services, ConfigMaps, RBAC, and monitoring resources.
4. Scale up and down.
5. Delete and recreate the cluster resource.

Success criteria:
- Resources are created and reconciled correctly.
- Scaling works without orphaned resources.
- RBAC is enforced.
- Cluster returns to healthy state after recreation.

## 1. Time-Series Engine

Context: Validate ingest and query workflows.

Steps:
1. Start coordinator and volumes.
2. Write samples with `POST /ts/write`.
3. Query with `POST /ts/query` using filters and time windows.
4. Verify aggregation behavior.

Success criteria:
- Samples are persisted and queryable.
- Aggregation and filters return expected results.

## 2. Vector Similarity Search

Context: Validate vector indexing and nearest-neighbor retrieval.

Steps:
1. Upsert vectors with `POST /vector/upsert`.
2. Query neighbors with `POST /vector/query`.
3. Check index stats with `GET /admin/vector/stats`.
4. Restart coordinator and verify results again.

Success criteria:
- Upserted vectors are returned by similarity queries.
- `top_k` behavior is respected.
- Index remains usable after restart.

## 3. Geo-Partitioning

Context: Validate routing across regions.

Steps:
1. Configure multiple regions.
2. Test latency, geo, round-robin, and primary routing strategies.
3. Simulate a regional outage and verify fallback.
4. Validate geo-fencing where configured.

Success criteria:
- Requests follow configured strategy.
- Failover is deterministic and safe.
- Geo-fencing rules are respected.

## 4. Data Tiering

Context: Validate movement across hot, warm, cold, and archive tiers.

Steps:
1. Start with tiering enabled.
2. Insert data with varied access patterns.
3. Trigger or wait for policy evaluation.
4. Verify tier transitions and reads.

Success criteria:
- Tier transitions follow policy.
- Data remains readable after movement.

## 5. io_uring Mode (Linux)

Context: Validate io_uring path and fallback behavior.

Steps:
1. Run on Linux with io_uring enabled.
2. Execute sustained read/write load.
3. Observe batching and throughput metrics.
4. Disable io_uring and verify fallback.

Success criteria:
- io_uring is active when available.
- Service remains functional on fallback.

## 6. Node Failure

Context: Validate availability during a volume outage.

Steps:
1. Start cluster (coordinator + 3 volumes).
2. Insert dataset.
3. Stop one volume.
4. Check `GET /health/live`, `GET /health/ready`, and `GET /metrics`.
5. Run reads and writes.
6. Restart failed volume.

Success criteria:
- Cluster remains available with degraded capacity.
- Recovered volume rejoins and catches up.

## 7. Split-Brain Resistance

Context: Validate consistency under network partition.

Steps:
1. Start cluster.
2. Partition connectivity between node groups.
3. Execute reads and writes in both partitions.
4. Observe leadership and replication.
5. Heal partition and validate convergence.

Success criteria:
- Single-leader safety is preserved.
- Cluster converges after healing.

## 8. Recovery After Failure

Context: Validate restart recovery for coordinator and volumes.

Steps:
1. Insert data.
2. Stop coordinator.
3. Validate expected client impact.
4. Restart coordinator.
5. Re-validate reads and writes.

Success criteria:
- Services recover cleanly.
- Data remains intact.

## 9. Stress and Load

Context: Validate sustained load behavior.

Steps:
1. Start cluster.
2. Run `bench/run_all.sh`.
3. Observe latency, throughput, and errors in `GET /metrics`.
4. Confirm no crash loops.

Success criteria:
- Cluster handles target load without instability.

## 10. Consistency Verification

Context: Validate replicated state after mixed operations.

Steps:
1. Insert deterministic key-value sets.
2. Execute update and delete operations.
3. Compare observed state across nodes.
4. Verify Raft replication alignment.

Success criteria:
- Final values are consistent across replicas.

## 11. Compaction and Repair Safety

Context: Validate admin operations under live traffic.

Steps:
1. Populate dataset.
2. Run `POST /admin/compact`.
3. Run `POST /admin/repair`.
4. Validate availability and data consistency.

Success criteria:
- No data loss.
- Cluster remains operational.

## 12. Audit Logging

Context: Validate traceability of sensitive actions.

Steps:
1. Ensure audit logging is enabled.
2. Perform admin actions and data operations.
3. Inspect audit outputs.

Success criteria:
- Sensitive actions are recorded with actor, target, and timestamp.

## 13. Persistent Storage Backends

Context: Validate durability with RocksDB or Sled.

Steps:
1. Configure backend.
2. Insert data.
3. Restart services.
4. Re-validate data access.

Success criteria:
- Data survives restart with no corruption.

## 14. Watch and Subscribe Notifications

Context: Validate real-time change propagation.

Steps:
1. Open WebSocket (`/watch/ws`) and SSE (`/watch/sse`) subscriptions.
2. Trigger put/delete operations.
3. Validate payload format and ordering.

Success criteria:
- Subscribers receive expected events promptly.
- No systematic duplicates or missed events.

## Execution Notes

- Record commands, timestamps, and environment.
- Capture logs and metrics for failures.
- Store outcomes in `tests/RESULT_TEMPLATE.md`.
