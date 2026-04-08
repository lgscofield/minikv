#!/usr/bin/env bash
# GA release preflight for minikv

set -euo pipefail

FAST=false
if [[ "${1:-}" == "--fast" ]]; then
  FAST=true
fi

echo "[release] minikv GA preflight"
echo "[release] mode: $([[ "$FAST" == "true" ]] && echo fast || echo full)"

run() {
  echo ""
  echo "[release] $*"
  "$@"
}

run cargo check --release
run cargo check --tests
run cargo fmt --all -- --check

if [[ "$FAST" == "false" ]]; then
  run cargo clippy --all-targets --all-features -- -D warnings
  run cargo test --all --release
else
  run cargo test --lib
fi

echo ""
echo "[release] preflight completed successfully"
