#!/usr/bin/env python3
"""End-to-end data science quickstart for minikv."""

from sdk.python.minikv_client import MinikvClient, to_pandas_points


def main() -> None:
    client = MinikvClient()

    print("== Vector demo ==")
    client.vector_upsert("signal-a", [0.12, 0.94, 0.21], {"kind": "signal", "region": "eu"})
    client.vector_upsert("signal-b", [0.11, 0.91, 0.25], {"kind": "signal", "region": "us"})
    client.vector_upsert("signal-c", [0.76, 0.14, 0.51], {"kind": "noise", "region": "eu"})

    nearest = client.vector_query([0.10, 0.90, 0.20], top_k=2)
    print(nearest)

    print("\n== Time-series demo ==")
    write_resp = client.timeseries_write(
        {
            "metric": "cpu.usage",
            "tags": {"host": "node-1", "env": "dev"},
            "points": [
                {"timestamp": 1710000000000, "value": 42.1},
                {"timestamp": 1710000060000, "value": 44.3},
            ],
        }
    )
    print("write:", write_resp)

    query_resp = client.timeseries_query(
        {
            "metric": "cpu.usage",
            "start": "2024-03-09T00:00:00Z",
            "end": "2024-03-10T00:00:00Z",
            "tags": {"host": "node-1"},
        }
    )
    print("query:", query_resp)

    try:
        df = to_pandas_points(query_resp)
        print("pandas rows:", len(df))
        print(df.head())
    except Exception as exc:  # pragma: no cover - optional dependency
        print("pandas conversion skipped:", exc)


if __name__ == "__main__":
    main()
