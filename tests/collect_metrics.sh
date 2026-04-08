#!/usr/bin/env bash

set -euo pipefail

usage() {
  echo "Usage: $0 [host:port] [output_dir]"
  echo "Example: $0 localhost:8080 ./metrics_logs"
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "Error: curl is required but was not found." >&2
  exit 1
fi

COORD_HOST="${1:-localhost:8080}"
OUTDIR="${2:-./metrics_logs}"
DATE="$(date +"%Y%m%d_%H%M%S")"

mkdir -p "$OUTDIR"

collect_endpoint() {
  local endpoint="$1"
  local output_file="$2"

  if curl -fsS "http://${COORD_HOST}${endpoint}" >"$output_file"; then
    echo "Collected ${endpoint} -> ${output_file}"
    return 0
  fi

  rm -f "$output_file"
  echo "Skipped ${endpoint} (unavailable or non-2xx)"
  return 1
}

echo "Collecting diagnostics from ${COORD_HOST}"

collect_endpoint "/metrics" "$OUTDIR/metrics_${DATE}.txt"
collect_endpoint "/health/live" "$OUTDIR/health_live_${DATE}.txt" || true
collect_endpoint "/health/ready" "$OUTDIR/health_ready_${DATE}.txt" || true
collect_endpoint "/admin/status" "$OUTDIR/admin_status_${DATE}.txt" || true
collect_endpoint "/admin/vector/stats" "$OUTDIR/vector_stats_${DATE}.txt" || true
collect_endpoint "/admin/raft_log" "$OUTDIR/raft_log_${DATE}.txt" || true

echo "Done. Artifacts available in ${OUTDIR}"
