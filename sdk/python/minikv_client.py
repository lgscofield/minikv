"""Notebook-first Python client for minikv.

This client is intentionally lightweight and focuses on the endpoints needed for
analytics and ML workflows:
- time-series write/query
- vector upsert/query
- range and batch access
- backup/restore and metrics access
- change streams via Server-Sent Events
"""

from __future__ import annotations

import json
import importlib
from dataclasses import dataclass
from typing import Any, Dict, Generator, Iterable, List, Optional

requests = None


@dataclass
class MinikvConfig:
    base_url: str = "http://localhost:8080"
    api_key: Optional[str] = None
    timeout_seconds: int = 15


class MinikvClient:
    def __init__(self, config: Optional[MinikvConfig] = None) -> None:
        global requests
        if requests is None:
            try:
                requests = importlib.import_module("requests")
            except ModuleNotFoundError as exc:
                raise RuntimeError(
                    "requests is required. Install dependencies with: "
                    "pip install -r sdk/python/requirements.txt"
                ) from exc

        if requests is None:
            raise RuntimeError(
                "requests is required. Install dependencies with: "
                "pip install -r sdk/python/requirements.txt"
            )
        self.config = config or MinikvConfig()
        self._session = requests.Session()
        self._session.headers.update({"Content-Type": "application/json"})
        if self.config.api_key:
            self._session.headers.update({"Authorization": f"Bearer {self.config.api_key}"})

    def _url(self, path: str) -> str:
        return f"{self.config.base_url.rstrip('/')}/{path.lstrip('/')}"

    def _json(self, method: str, path: str, **kwargs: Any) -> Dict[str, Any]:
        resp = self._session.request(
            method=method,
            url=self._url(path),
            timeout=self.config.timeout_seconds,
            **kwargs,
        )
        resp.raise_for_status()
        return resp.json()

    # -----------------------------
    # Time-series
    # -----------------------------
    def timeseries_write(self, payload: Dict[str, Any]) -> Dict[str, Any]:
        return self._json("POST", "/ts/write", json=payload)

    def timeseries_query(self, payload: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        payload = payload or {}
        return self._json("POST", "/ts/query", json=payload)

    # -----------------------------
    # Vector embeddings
    # -----------------------------
    def vector_upsert(
        self,
        item_id: str,
        vector: List[float],
        metadata: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        payload = {
            "id": item_id,
            "values": vector,
            "metadata": metadata,
        }
        return self._json("POST", "/vector/upsert", json=payload)

    def vector_query(self, vector: List[float], top_k: int = 10) -> Dict[str, Any]:
        payload = {
            "vector": vector,
            "top_k": top_k,
        }
        return self._json("POST", "/vector/query", json=payload)

    # -----------------------------
    # KV analytics helpers
    # -----------------------------
    def range_query(
        self,
        start: str,
        end: str,
        include_values: bool = False,
    ) -> Dict[str, Any]:
        params = {
            "start": start,
            "end": end,
            "include_values": str(include_values).lower(),
        }
        return self._json("GET", "/range", params=params)

    def batch(self, ops: Iterable[Dict[str, Any]]) -> Dict[str, Any]:
        return self._json("POST", "/batch", json={"ops": list(ops)})

    # -----------------------------
    # Change streams
    # -----------------------------
    def watch_sse(self) -> Generator[Dict[str, Any], None, None]:
        with self._session.get(
            self._url("/watch/sse"),
            stream=True,
            timeout=self.config.timeout_seconds,
        ) as response:
            response.raise_for_status()
            for raw_line in response.iter_lines(decode_unicode=True):
                if not raw_line:
                    continue
                line = raw_line.strip()
                if not line.startswith("data:"):
                    continue
                payload = line[len("data:") :].strip()
                if not payload:
                    continue
                try:
                    yield json.loads(payload)
                except json.JSONDecodeError:
                    continue

    # -----------------------------
    # Ops helpers
    # -----------------------------
    def metrics_text(self) -> str:
        resp = self._session.get(self._url("/metrics"), timeout=self.config.timeout_seconds)
        resp.raise_for_status()
        return resp.text

    def backup(self, backup_type: str = "full") -> Dict[str, Any]:
        return self._json("POST", "/admin/backup", json={"type": backup_type})

    def list_backups(self) -> Dict[str, Any]:
        return self._json("GET", "/admin/backups")

    def restore(self, backup_id: str, target_path: Optional[str] = None) -> Dict[str, Any]:
        payload: Dict[str, Any] = {"backup_id": backup_id}
        if target_path:
            payload["target_path"] = target_path
        return self._json("POST", "/admin/restore", json=payload)


def to_pandas_points(query_result: Dict[str, Any]):
    pd = importlib.import_module("pandas")

    points = query_result.get("points", [])
    return pd.DataFrame(points)


def to_polars_points(query_result: Dict[str, Any]):
    pl = importlib.import_module("polars")

    points = query_result.get("points", [])
    return pl.DataFrame(points)


def to_arrow_points(query_result: Dict[str, Any]):
    pa = importlib.import_module("pyarrow")

    points = query_result.get("points", [])
    return pa.Table.from_pylist(points)
