# Professional Test Report Template – minikv v0.9.0 (Current Release)
# (Updated for v0.9.0: K8s Operator, Time-series, Geo, Tiering, io_uring)

Use this template to record the results of each manual test scenario. Complete each section during test execution to ensure traceability and reproducibility.

---

## General Information
- **Date:**
- **Tester:**
- **Cluster Version:**
- **Configuration (nodes, volumes, options):**

---

## Scenario Tested
- **Scenario Name:**
- **Objective:**
- **Pre-conditions:**

---

## Steps Executed
1. 
2. 
3. 
...

---

## Commands Used
- 
- 
- 

---

## Verification Points
- 
- 
- 

---

## Results Observed
- 
- 
- 

---

## Metrics and Logs
- **/metrics extracts:**
- **Raft logs:**
- **Other:**

---

## Success Criteria
- [ ] Success
- [ ] Failure
- **Comments:**

---

## Screenshots / Evidence
- (Attach files, screenshots, log extracts)

---



## Additional Checks for v0.9.0

### Kubernetes Operator & Cloud-Native
- [ ] MiniKVCluster CRD deployed and recognized
- [ ] Autoscaling (HPA) functional
- [ ] RBAC and permissions correct
- [ ] StatefulSet, ConfigMap, ServiceMonitor created
- [ ] Deployment via basic-cluster.yaml/production-cluster.yaml successful

### Time-Series Engine
- [ ] /ts/write and /ts/query endpoints functional
- [ ] Compression, downsampling, aggregations tested
- [ ] Retention and multiple resolutions verified

### Geo-Partitioning
- [ ] Region-based routing (latency, geo, round-robin, primary) tested
- [ ] Geo-fencing and compliance (GDPR) verified
- [ ] Automatic failover between regions

### Data Tiering
- [ ] Automatic movement between hot/warm/cold/archive
- [ ] Tiering policies applied (age, access, size)
- [ ] Per-tier compression and S3 archive tested

### io_uring (Linux)
- [ ] io_uring mode enabled and detected
- [ ] Zero-copy and batching operational
- [ ] Fallback to standard I/O if not supported

### Modules & Integration
- [ ] k8s_operator, timeseries, geo, tiering, io_uring present and active
- [ ] Integration tests (timeseries, geo, tiering) passed

### Security, Multi-tenancy, Observability (reminders v0.6.0+)
- [ ] Audit Logging: all admin/sensitive actions logged
- [ ] Persistent backend: data survives restarts
- [ ] Watch/Subscribe: real-time notifications, no loss/duplicates


---



## Real-time Notification Verification
- **WebSocket/SSE endpoint tested:**
- **Events received:**
- **Event content (sample):**
- [ ] All expected events received
- [ ] No duplicate or missing events

## Time-Series/Geo/Tiering/Operator Evidence (v0.9.0)
- **/admin/timeseries/stats:**
- **/admin/geo/status:**
- **Tiering stats:**
- **Operator logs/events:**

> This report should be archived for each scenario to ensure quality, compliance, and traceability of the minikv v0.9.0 release.

---

> This report should be archived for each scenario execution to ensure quality and compliance of the minikv cluster.
