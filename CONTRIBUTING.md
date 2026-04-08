# Contributing to minikv

Thanks for contributing. This guide is aligned with the v1.0.0 GA workflow.

## Ways to Contribute

- Report bugs with clear reproduction steps.
- Propose roadmap or UX improvements.
- Submit code, tests, docs, and runbook improvements.
- Improve observability, release engineering, and operator workflows.

Issues: https://github.com/whispem/minikv/issues

## Local Setup

```bash
git clone https://github.com/whispem/minikv
cd minikv
make build
```

Recommended checks before opening a PR:

```bash
make test
make fmt
make clippy
make release-preflight
```

## Branch and Commit Workflow

1. Create a branch.

```bash
git checkout -b feat/short-description
```

2. Implement changes with tests.
3. Run formatting/lint/tests.
4. Update docs if behavior or APIs changed.
5. Commit with a clear message.

Examples:

- `feat(timeseries): add tag-filtered query validation`
- `fix(vector): persist index atomically`
- `docs(release): update preflight instructions`

## Pull Request Checklist

- Feature behavior is tested (unit and/or integration).
- `cargo fmt --all` is clean.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- Relevant docs updated (`README.md`, `CHANGELOG.md`, runbooks).
- Any API/endpoint changes are documented with examples.

## Testing Expectations

Minimum for most PRs:

```bash
cargo test --lib
```

For API, storage, replication, or release-impacting changes, run:

```bash
make test
make release-preflight
```

For release-critical PRs, also run:

```bash
make release-preflight-full
```

## Documentation Requirements

Update docs when you change behavior in any of these areas:

- Public APIs/endpoints
- Operational procedures (backup/restore, observability, deployment)
- Release process
- Developer commands or workflows

Key files:

- `README.md`
- `CHANGELOG.md`
- `docs/ops-backup-restore.md`
- `docs/release-engineering-v1.0.0.md`

## Current Priority Areas

- CDC integrations (Kafka Connect templates)
- Read replicas for analytics traffic
- Vector indexing acceleration (HNSW/PQ)
- PITR and disaster recovery hardening
- Multi-region failover automation

## Code Style

- Keep changes small and focused.
- Prefer explicit errors over hidden fallbacks.
- Write doc comments for non-trivial behavior.
- Preserve backward compatibility unless the PR clearly documents a breaking change.

## Community

- Be respectful and constructive.
- Assume good intent.
- Review code for correctness, maintainability, and operational risk.

Thanks for helping make minikv more reliable and more useful.
