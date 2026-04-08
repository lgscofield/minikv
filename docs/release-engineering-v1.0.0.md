# Release Engineering - v1.0.0

## Objective

Ship a verifiable, reproducible v1.0.0 release candidate and promote it to GA.

## 1. Versioning

- Set Cargo version to 1.0.0.
- Ensure changelog has a v1.0.0 entry.

## 2. Preflight Checks

Fast preflight:

```bash
make release-preflight
```

Full preflight:

```bash
make release-preflight-full
```

## 3. API Smoke Validation

- Coordinator liveness: `GET /health/live`
- Coordinator readiness: `GET /health/ready`
- Time-series write/query: `POST /ts/write`, `POST /ts/query`
- Vector index: `POST /vector/upsert`, `POST /vector/query`, `GET /admin/vector/stats`
- Metrics endpoint: `GET /metrics`

## 4. Operational Readiness

- Validate observability stack startup under `opentelemetry/`.
- Validate alert rules are loaded in Prometheus.
- Validate Grafana dashboard provisioning.
- Run backup/restore drill following runbook:
  - docs/ops-backup-restore.md

## 5. Release Artifacts

- Build release binaries:

```bash
cargo build --release
```

- Optional Docker build:

```bash
make docker-build
```

## 6. Tag and Publish

```bash
git add -A
git commit -m "release: v1.0.0 GA"
git tag -a v1.0.0 -m "minikv v1.0.0"
git push origin main --tags
```

## 7. Go/No-Go Criteria

Go:
- preflight green
- critical smoke endpoints green
- backup/restore drill green
- no critical open bugs

No-Go:
- failing preflight
- data-loss or restore failure
- failing readiness/liveness under normal operation
