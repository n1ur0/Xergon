# Load Test Harness

Async load tester for the Xergon Network relay. Uses `aiohttp` for concurrency and `rich` for terminal output.

## Quick Start

```bash
# Run everything with defaults (http://localhost:8080, 50 concurrent, 1000 requests)
./run.sh

# Or manually:
python3 load_test.py

# Target a different host
python3 load_test.py --target http://relay.example.com:8080

# Run only the inference scenario
python3 load_test.py --scenario inference --concurrent 100 --requests 5000

# Save results as JSON for CI
python3 load_test.py --output results.json
```

## Scenarios

| Scenario     | Description                                              |
|-------------|----------------------------------------------------------|
| inference   | POST `/v1/chat/completions` — measures latency/throughput |
| models      | GET `/v1/models` — concurrent list requests              |
| rate-limit  | Rapid-fire requests, verifies 429 responses              |
| health      | Continuous `/health` and `/ready` polling                |
| all         | Runs every scenario (default)                            |

## CLI Options

| Flag            | Default                    | Description                     |
|-----------------|----------------------------|---------------------------------|
| `--target`      | `http://localhost:8080`    | Relay base URL                  |
| `--scenario`    | `all`                      | Scenario to run                 |
| `--concurrent`  | `50`                       | Concurrent connections          |
| `--requests`    | `1000`                     | Max requests per scenario       |
| `--duration`    | `60`                       | Max seconds per scenario        |
| `--output`      | (none)                     | Write JSON results to file      |

## JSON Output

When `--output results.json` is used, the output is an array of per-scenario objects:

```json
[
  {
    "name": "inference",
    "total_requests": 1000,
    "successful": 980,
    "failed": 20,
    "p50_ms": 142.3,
    "p95_ms": 312.1,
    "p99_ms": 502.8,
    "mean_ms": 155.7,
    "throughput": 49.0,
    "error_rate": 0.02,
    "error_breakdown": {"HTTP 500": 15, "HTTP 503": 5}
  }
]
```

## Requirements

- Python 3.10+
- `aiohttp >= 3.9`
- `rich >= 13.0`
