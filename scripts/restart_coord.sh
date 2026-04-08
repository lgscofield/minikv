#!/usr/bin/env bash

set -euo pipefail

HOST_PORT_HTTP="${HOST_PORT_HTTP:-8000}"
HOST_PORT_GRPC="${HOST_PORT_GRPC:-8001}"
BIN="${BIN:-minikv-coord}"
MODE="${MODE:-release}"
NODE_ID="${NODE_ID:-1}"

pids="$(lsof -ti :"${HOST_PORT_HTTP}" -ti :"${HOST_PORT_GRPC}" || true)"
if [[ -n "${pids}" ]]; then
  echo "Stopping existing process(es): ${pids}"
  kill ${pids} || true
  sleep 1
fi

if [[ "${MODE}" == "release" ]]; then
  echo "Starting ${BIN} (release) on ports ${HOST_PORT_HTTP}/${HOST_PORT_GRPC}..."
  exec cargo run --bin "${BIN}" --release -- serve --id "${NODE_ID}"
else
  echo "Starting ${BIN} (debug) on ports ${HOST_PORT_HTTP}/${HOST_PORT_GRPC}..."
  exec cargo run --bin "${BIN}" -- serve --id "${NODE_ID}"
fi
