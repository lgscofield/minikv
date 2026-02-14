# 🦀 minikv

**A distributed, multi-tenant key-value & object store written in Rust**

minikv provides strong consistency (Raft + 2PC), durability (WAL), and production-grade observability, security, and multi-tenancy — all in a modern Rust codebase.

Built in public as a learning-by-doing project — now evolved into a complete, reference implementation of distributed systems in Rust.

[![Repo](https://img.shields.io/badge/github-whispem%2Fminikv-blue)](https://github.com/whispem/minikv)
[![Rust](https://img.shields.io/badge/rust-1.81+-orange.svg)](https://rustup.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Production Grade](https://img.shields.io/badge/status-production_grade-success)](https://github.com/whispem/minikv)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)](.github/workflows/ci.yml)

---

## 🚦 What's New in v0.9.0


minikv v0.9.0 introduces cloud-native and performance features:

- **Kubernetes Operator:** CRD-based cluster management with autoscaling
- **Time-series engine:** Compression, downsampling, aggregations
- **Geo-partitioning:** Data locality and compliance (GDPR)
- **Data tiering:** Hot/warm/cold/archive automatic data movement
- **io_uring:** Zero-copy I/O for Linux (optional)

Previous highlights (v0.8.0): cross-DC replication, CDC, admin UI, backup/restore, plugins.

---

## 📚 Table of Contents

- [What is minikv?](#what-is-minikv)
- [Tech Stack](#tech-stack)
- [Quick Start](#quick-start)
- [Architecture](#architecture)
- [Performance](#performance)
- [Features](#features)
- [Roadmap](#roadmap)
- [Story](#story)
- [Documentation](#documentation)
- [Development](#development)
- [Contributing](#contributing)
- [Contact](#contact)

---

## 🤔 What is minikv?

minikv is a distributed key-value store written in [Rust](https://www.rust-lang.org/), designed for simplicity, speed, and reliability.

**Who is this for ?**  
minikv is for engineers learning distributed systems, teams experimenting with Rust-based infrastructure, and anyone curious about consensus, durability, and system trade-offs.

- **Clustered :** Raft consensus and 2PC for transactional writes
- **Virtual Sharding :** 256 vshards for elastic scaling & balancing
- **WAL :** Write-ahead log for durability
- **gRPC** for node communication, **HTTP REST & S3 API** for clients
- **Bloom filters, snapshots, watch/subscribe** for performance & reactivity

---

## 🛠 Tech Stack

| Layer | Technology |
|-------|------------|
| Language | Rust 1.81+ |
| Async Runtime | Tokio |
| Consensus | Raft (tikv/raft-rs) |
| RPC | gRPC (Tonic), Protobuf |
| HTTP | Axum 0.7, Tower |
| Storage | RocksDB, Sled, in-memory |
| Serialization | serde, bincode, JSON |
| Crypto | AES-256-GCM, Argon2, BLAKE3 |
| Compression | LZ4, Zstd, Snappy, Gzip |
| Observability | Prometheus, OpenTelemetry |
| TLS | rustls |
| Benchmarks | k6 (JavaScript) |
| Build | Cargo, Make |
| Deploy | Docker, Kubernetes |

---

## ⚡ Quick Start

```bash
git clone https://github.com/whispem/minikv.git
cd minikv
cargo build --release

# Start a node
cargo run -- --config config.example.toml

# API examples
curl localhost:8080/health/ready   # readiness
curl localhost:8080/metrics        # Prometheus metrics
curl localhost:8080/admin/status   # admin dashboard

# Create API key (admin)
curl -X POST http://localhost:8080/admin/keys -d '{"role":"ReadWrite","tenant_id":"acme"}'

# S3 (demo)
curl -X PUT localhost:8080/s3/mybucket/mykey -d 'hello minikv!'
curl localhost:8080/s3/mybucket/mykey
```
For cluster setup and advanced options, see the [documentation](#documentation).

---

## 📐 Architecture

```
┌─────────────────────────────────────────────────────┐
│                       Clients                       │
│          REST │ S3 │ gRPC │ WS            │
└──────────────────────────┬──────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────┐
│                  Coordinators                        │
│    ┌───────┐  ┌───────┐  ┌───────┐                 │
│    │ Raft  │──│ Raft  │──│ Raft  │  (3-5 nodes)   │
│    │Leader │  │Follower│  │Follower│                 │
│    └───────┘  └───────┘  └───────┘                 │
└──────────────────────────┬──────────────────────────┘
                           │ Metadata + Routing
┌──────────────────────────▼──────────────────────────┐
│                  Volume Servers                      │
│    ┌───────┐  ┌───────┐  ┌───────┐                 │
│    │Volume1│  │Volume2│  │Volume3│  (N nodes)      │
│    │Shards │  │Shards │  │Shards │                 │
│    │0-85   │  │86-170 │  │171-255│                 │
│    └───┬───┘  └───┬───┘  └───┬───┘                 │
│        ▼          ▼          ▼                     │
│    ┌───────┐  ┌───────┐  ┌───────┐                 │
│    │RocksDB│  │ Sled  │  │Memory │                 │
│    └───────┘  └───────┘  └───────┘                 │
└─────────────────────────────────────────────────────┘
```

| Component | Description |
|-----------|-------------|
| Coordinator | Raft consensus, metadata, routing, 2PC |
| Volume Server | Data storage, replication, compaction |
| Virtual Shards | 256 shards for distribution |
| WAL | Write-ahead log for durability |

---

## 🚀 Performance

- Write throughput : over 50,000 operations/sec (single node, in-memory)
- Sub-millisecond read latency
- Cluster tested (3–5 nodes, commodity VMs)
- Built-in Prometheus metrics

---

## 🌟 Features

### Consensus & Distribution
- Raft consensus (leader election, log replication)
- Two-phase commit (2PC) for multi-key transactions
- 256 virtual shards with consistent hashing
- Automatic rebalancing and failover
- Cross-datacenter async replication
- Conflict resolution (LWW, vector clocks)

### Storage
- Pluggable backends: RocksDB, Sled, in-memory
- Write-ahead log (WAL)
- Bloom filters for fast lookups
- LZ4/Zstd compression
- Data tiering (hot/warm/cold/archive)
- Compaction and garbage collection


### Time-Series (v0.9.0)
- Dedicated time-series engine
- Multiple resolutions (raw, 1min, 5min, 1h, 1day)
- Automatic downsampling
- Delta and Gorilla compression
- Aggregations: sum, avg, min, max, count, stddev

### Geo-Partitioning (v0.9.0)
- Region-aware data placement
- Routing: nearest, primary, round-robin, geo-fenced
- GDPR/data residency compliance
- Automatic failover between regions

### APIs
- **HTTP REST:** CRUD, batch, range, prefix queries
- **S3-compatible:** Buckets, objects, multipart upload
- **gRPC:** Internal node communication
- **WebSocket/SSE:** Real-time watch/subscribe

### Security
- API keys with Argon2 hashing
- JWT authentication
- Role-based access control (RBAC)
- Multi-tenant isolation
- AES-256-GCM encryption at rest
- TLS for HTTP and gRPC
- Audit logging

### Multi-tenancy
- Tenant isolation
- Per-tenant quotas (storage, requests)
- Rate limiting (token bucket)

### Observability
- Prometheus metrics
- OpenTelemetry tracing
- Structured logging
- Admin Web UI dashboard
- Health probes (/health/ready, /health/live)

### Operations
- Backup & restore (full, incremental)
- Change Data Capture (CDC)
- Plugin system (storage, auth, hooks)
- Kubernetes Operator (v0.9.0)
- io_uring zero-copy I/O (Linux, v0.9.0)

---

## 🗺️ Roadmap

### Kubernetes (v0.9.0)

```yaml
apiVersion: minikv.io/v1alpha1
kind: MiniKVCluster
metadata:
  name: my-cluster
spec:
  coordinators:
    replicas: 3
  volumes:
    replicas: 3
    replicationFactor: 3
  security:
    tls:
      enabled: true
  autoscaling:
    enabled: true
    maxReplicas: 10
```

Deploy with:
```bash
kubectl apply -f k8s/crds/minikvcluster.yaml
kubectl apply -f k8s/examples/basic-cluster.yaml
```

---

## 📍 Versions

### v0.9.0 (current)
- Kubernetes Operator (CRD, autoscaling)
- Time-series engine
- Geo-partitioning
- Data tiering
- io_uring (Linux)

---

## 🗺️ Roadmap

### v1.0.0
- [ ] MiniQL query optimizer
- [ ] Full-text search (tantivy)
- [ ] Helm chart
- [ ] Connection pooling

### v1.1.0
- [ ] Vector embeddings storage
- [ ] Distributed transactions (Percolator)
- [ ] Read replicas
- [ ] Multi-region active-active

### v1.2.0
- [ ] Serverless mode (scale to zero)
- [ ] WebAssembly UDFs
- [ ] Change streams (Kafka Connect)
- [ ] Point-in-time recovery

### Future
- [ ] GPU-accelerated queries
- [ ] Raft learner nodes
- [ ] Auto-sharding (split/merge)
- [ ] Global secondary indexes
- [ ] CockroachDB-style SQL layer

---


## 🛠️ Development

```bash
cargo build          # Build
cargo test           # Tests
cargo clippy         # Lint
cargo fmt            # Format
```

### Project Structure

```
src/
├── bin/              # Binaries (coord, volume, cli)
├── common/           # Shared modules
│   ├── raft.rs       # Consensus
│   ├── storage.rs    # Storage backends
│   ├── auth.rs       # Authentication
│   ├── encryption.rs # Encryption at rest
│   ├── replication.rs# Cross-DC replication
│   ├── cdc.rs        # Change Data Capture
│   ├── backup.rs     # Backup/restore
│   ├── plugin.rs     # Plugin system
│   ├── timeseries.rs # Time-series
│   ├── geo.rs        # Geo-partitioning
│   ├── tiering.rs    # Data tiering
│   ├── io_uring.rs   # io_uring backend
│   └── io_uring.rs   # io_uring backend
├── coordinator/      # Coordinator logic
├── volume/           # Volume server
└── ops/              # Operational tools
k8s/                  # Kubernetes manifests
bench/                # k6 benchmarks
```

---

## 🤝 Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

---

## 📬 Contact

GitHub: [whispem/minikv](https://github.com/whispem/minikv)

---

**MIT License** - See [LICENSE](LICENSE)
