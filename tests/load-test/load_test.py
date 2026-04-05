#!/usr/bin/env python3
"""Async load tester for Xergon Network relay (OpenAI-compatible endpoints)."""

from __future__ import annotations

import argparse
import asyncio
import json
import statistics
import sys
import time
from dataclasses import dataclass, field, asdict
from typing import Optional

import aiohttp
from rich.console import Console
from rich.progress import (
    Progress,
    SpinnerColumn,
    BarColumn,
    TextColumn,
    TimeElapsedColumn,
    MofNCompleteColumn,
    TaskProgressColumn,
)
from rich.table import Table
from rich.panel import Panel
from rich.text import Text

console = Console()

# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------

@dataclass
class RequestResult:
    status: int
    latency_ms: float
    error: Optional[str] = None


@dataclass
class ScenarioResult:
    name: str
    total_requests: int = 0
    successful: int = 0
    failed: int = 0
    latencies: list[float] = field(default_factory=list)
    error_breakdown: dict[str, int] = field(default_factory=dict)
    duration_s: float = 0.0

    # derived
    @property
    def error_rate(self) -> float:
        return self.failed / self.total_requests if self.total_requests else 0.0

    @property
    def throughput(self) -> float:
        return self.successful / self.duration_s if self.duration_s else 0.0

    def percentile(self, p: float) -> float:
        if not self.latencies:
            return 0.0
        s = sorted(self.latencies)
        k = (len(s) - 1) * (p / 100.0)
        lo = int(k)
        hi = min(lo + 1, len(s) - 1)
        frac = k - lo
        return s[lo] + frac * (s[hi] - s[lo])

    def to_dict(self) -> dict:
        d = asdict(self)
        d["throughput"] = round(self.throughput, 2)
        d["error_rate"] = round(self.error_rate, 4)
        d["p50_ms"] = round(self.percentile(50), 2)
        d["p95_ms"] = round(self.percentile(95), 2)
        d["p99_ms"] = round(self.percentile(99), 2)
        d["mean_ms"] = round(statistics.mean(self.latencies), 2) if self.latencies else 0.0
        d["min_ms"] = round(min(self.latencies), 2) if self.latencies else 0.0
        d["max_ms"] = round(max(self.latencies), 2) if self.latencies else 0.0
        return d


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _make_inference_payload() -> dict:
    return {
        "model": "gpt-4o-mini",
        "messages": [
            {"role": "system", "content": "You are a helpful assistant. Be brief."},
            {"role": "user", "content": "Say hello in one word."},
        ],
        "max_tokens": 16,
        "temperature": 0,
    }


async def _do_request(
    session: aiohttp.ClientSession,
    method: str,
    url: str,
    *,
    json_body: Optional[dict] = None,
    timeout: float = 30.0,
) -> RequestResult:
    t0 = time.monotonic()
    try:
        async with session.request(
            method, url, json=json_body, timeout=aiohttp.ClientTimeout(total=timeout)
        ) as resp:
            await resp.read()
            elapsed = (time.monotonic() - t0) * 1000
            return RequestResult(status=resp.status, latency_ms=elapsed)
    except asyncio.CancelledError:
        raise
    except aiohttp.ClientConnectorError as exc:
        elapsed = (time.monotonic() - t0) * 1000
        return RequestResult(status=0, latency_ms=elapsed, error=f"connection refused: {exc}")
    except aiohttp.ClientError as exc:
        elapsed = (time.monotonic() - t0) * 1000
        return RequestResult(status=0, latency_ms=elapsed, error=str(exc))
    except Exception as exc:
        elapsed = (time.monotonic() - t0) * 1000
        return RequestResult(status=0, latency_ms=elapsed, error=str(exc))


# ---------------------------------------------------------------------------
# Scenarios
# ---------------------------------------------------------------------------

async def scenario_inference(
    target: str, concurrent: int, max_requests: int, duration: int
) -> ScenarioResult:
    """POST /v1/chat/completions with concurrent users."""
    url = f"{target}/v1/chat/completions"
    result = ScenarioResult(name="inference")
    semaphore = asyncio.Semaphore(concurrent)
    completed = asyncio.Event()
    counter = {"n": 0}
    deadline = time.monotonic() + duration

    console.print(f"[bold cyan]Inference scenario[/]  concurrent={concurrent}  max_requests={max_requests}  duration={duration}s")

    async def worker(session: aiohttp.ClientSession):
        while not completed.is_set():
            if counter["n"] >= max_requests or time.monotonic() >= deadline:
                completed.set()
                return
            async with semaphore:
                if counter["n"] >= max_requests or time.monotonic() >= deadline:
                    completed.set()
                    return
                counter["n"] += 1
                r = await _do_request(session, "POST", url, json_body=_make_inference_payload())
                result.total_requests += 1
                result.latencies.append(r.latency_ms)
                if r.error or r.status >= 400:
                    result.failed += 1
                    key = f"HTTP {r.status}" if r.status else r.error
                    result.error_breakdown[key] = result.error_breakdown.get(key, 0) + 1
                else:
                    result.successful += 1

    t0 = time.monotonic()
    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        BarColumn(),
        TaskProgressColumn(),
        TimeElapsedColumn(),
        console=console,
    ) as progress:
        task = progress.add_task("Sending inference requests...", total=max_requests)
        async with aiohttp.ClientSession() as session:
            workers = [asyncio.create_task(worker(session)) for _ in range(concurrent)]
            while not completed.is_set() and counter["n"] < max_requests:
                progress.update(task, completed=counter["n"])
                await asyncio.sleep(0.2)
            progress.update(task, completed=counter["n"])
            completed.set()
            await asyncio.gather(*workers, return_exceptions=True)

    result.duration_s = time.monotonic() - t0
    return result


async def scenario_models(
    target: str, concurrent: int, max_requests: int, duration: int
) -> ScenarioResult:
    """GET /v1/models concurrent requests."""
    url = f"{target}/v1/models"
    result = ScenarioResult(name="models")
    semaphore = asyncio.Semaphore(concurrent)
    completed = asyncio.Event()
    counter = {"n": 0}
    deadline = time.monotonic() + duration

    console.print(f"[bold cyan]Models scenario[/]  concurrent={concurrent}  max_requests={max_requests}  duration={duration}s")

    async def worker(session: aiohttp.ClientSession):
        while not completed.is_set():
            if counter["n"] >= max_requests or time.monotonic() >= deadline:
                completed.set()
                return
            async with semaphore:
                if counter["n"] >= max_requests or time.monotonic() >= deadline:
                    completed.set()
                    return
                counter["n"] += 1
                r = await _do_request(session, "GET", url)
                result.total_requests += 1
                result.latencies.append(r.latency_ms)
                if r.error or r.status >= 400:
                    result.failed += 1
                    key = f"HTTP {r.status}" if r.status else r.error
                    result.error_breakdown[key] = result.error_breakdown.get(key, 0) + 1
                else:
                    result.successful += 1

    t0 = time.monotonic()
    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        BarColumn(),
        TaskProgressColumn(),
        TimeElapsedColumn(),
        console=console,
    ) as progress:
        task = progress.add_task("Sending model list requests...", total=max_requests)
        async with aiohttp.ClientSession() as session:
            workers = [asyncio.create_task(worker(session)) for _ in range(concurrent)]
            while not completed.is_set() and counter["n"] < max_requests:
                progress.update(task, completed=counter["n"])
                await asyncio.sleep(0.2)
            progress.update(task, completed=counter["n"])
            completed.set()
            await asyncio.gather(*workers, return_exceptions=True)

    result.duration_s = time.monotonic() - t0
    return result


async def scenario_rate_limit(
    target: str, concurrent: int, max_requests: int, duration: int
) -> ScenarioResult:
    """Rapid-fire requests from one client; expect 429s."""
    url = f"{target}/v1/models"
    result = ScenarioResult(name="rate-limit")
    rate_limited = 0

    console.print(f"[bold cyan]Rate-limit scenario[/]  sending {max_requests} rapid requests...")

    t0 = time.monotonic()
    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        BarColumn(),
        MofNCompleteColumn(),
        TimeElapsedColumn(),
        console=console,
    ) as progress:
        task = progress.add_task("Flooding requests...", total=max_requests)
        async with aiohttp.ClientSession() as session:
            for i in range(max_requests):
                if time.monotonic() - t0 > duration:
                    break
                r = await _do_request(session, "GET", url, timeout=10.0)
                result.total_requests += 1
                result.latencies.append(r.latency_ms)
                if r.status == 429:
                    rate_limited += 1
                    result.successful += 1  # 429 is expected
                elif r.error or r.status >= 400:
                    result.failed += 1
                    key = f"HTTP {r.status}" if r.status else r.error
                    result.error_breakdown[key] = result.error_breakdown.get(key, 0) + 1
                else:
                    result.successful += 1
                progress.update(task, advance=1)

    result.duration_s = time.monotonic() - t0
    result.error_breakdown["429 responses received"] = rate_limited
    result.successful = rate_limited  # count only 429s as "correct"
    result.failed = result.total_requests - rate_limited
    return result


async def scenario_health(
    target: str, concurrent: int, max_requests: int, duration: int
) -> ScenarioResult:
    """Continuous /health and /ready polling."""
    endpoints = ["/health", "/ready"]
    result = ScenarioResult(name="health")
    counter = {"n": 0}

    console.print(f"[bold cyan]Health scenario[/]  polling for {duration}s...")

    t0 = time.monotonic()
    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        BarColumn(),
        TimeElapsedColumn(),
        console=console,
    ) as progress:
        task = progress.add_task("Polling /health and /ready...", total=duration)
        async with aiohttp.ClientSession() as session:
            while time.monotonic() - t0 < duration:
                for ep in endpoints:
                    r = await _do_request(session, "GET", f"{target}{ep}", timeout=5.0)
                    result.total_requests += 1
                    counter["n"] += 1
                    result.latencies.append(r.latency_ms)
                    if r.error or r.status >= 400:
                        result.failed += 1
                        key = f"HTTP {r.status}" if r.status else r.error
                        result.error_breakdown[key] = result.error_breakdown.get(key, 0) + 1
                    else:
                        result.successful += 1
                progress.update(task, completed=int(time.monotonic() - t0))

    result.duration_s = time.monotonic() - t0
    return result


# ---------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------

def print_report(results: list[ScenarioResult]) -> None:
    console.print()
    console.rule("[bold green]Load Test Report[/]")

    for r in results:
        panel_lines = [
            f"  Total Requests : {r.total_requests}",
            f"  Successful     : {r.successful}",
            f"  Failed         : {r.failed}",
            f"  Error Rate     : {r.error_rate:.2%}",
            f"  Duration       : {r.duration_s:.2f}s",
            f"  Throughput     : {r.throughput:.2f} req/s",
            "",
            "  Latency (ms):",
            f"    min  : {r.percentile(0):.2f}",
            f"    p50  : {r.percentile(50):.2f}",
            f"    p95  : {r.percentile(95):.2f}",
            f"    p99  : {r.percentile(99):.2f}",
            f"    mean : {statistics.mean(r.latencies):.2f}" if r.latencies else "    mean : N/A",
            f"    max  : {max(r.latencies):.2f}" if r.latencies else "    max  : N/A",
        ]

        if r.error_breakdown:
            panel_lines.append("")
            panel_lines.append("  Error Breakdown:")
            for k, v in r.error_breakdown.items():
                panel_lines.append(f"    {k}: {v}")

        console.print(Panel("\n".join(panel_lines), title=f"[bold]{r.name}[/]", border_style="blue"))

    # Summary table
    table = Table(title="Summary")
    table.add_column("Scenario", style="bold")
    table.add_column("Requests", justify="right")
    table.add_column("Errors", justify="right")
    table.add_column("Error %", justify="right")
    table.add_column("p50 (ms)", justify="right")
    table.add_column("p95 (ms)", justify="right")
    table.add_column("p99 (ms)", justify="right")
    table.add_column("Throughput", justify="right")

    for r in results:
        err_pct = f"{r.error_rate:.1%}"
        table.add_row(
            r.name,
            str(r.total_requests),
            str(r.failed),
            err_pct,
            f"{r.percentile(50):.1f}",
            f"{r.percentile(95):.1f}",
            f"{r.percentile(99):.1f}",
            f"{r.throughput:.1f} req/s",
        )

    console.print(table)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

SCENARIOS = {
    "inference": scenario_inference,
    "models": scenario_models,
    "rate-limit": scenario_rate_limit,
    "health": scenario_health,
}


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description="Xergon Network load tester",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--target", default="http://localhost:8080", help="Base URL (default: http://localhost:8080)")
    p.add_argument(
        "--scenario",
        choices=["inference", "models", "rate-limit", "health", "all"],
        default="all",
        help="Scenario to run (default: all)",
    )
    p.add_argument("--concurrent", type=int, default=50, help="Concurrent users (default: 50)")
    p.add_argument("--requests", type=int, default=1000, help="Max requests per scenario (default: 1000)")
    p.add_argument("--duration", type=int, default=60, help="Max duration in seconds (default: 60)")
    p.add_argument("--output", type=str, default=None, help="Write JSON results to FILE")
    return p


async def main() -> int:
    args = build_parser().parse_args()

    # Quick connectivity check
    console.print(f"[bold]Xergon Network Load Tester[/]")
    console.print(f"  Target    : {args.target}")
    console.print(f"  Scenario  : {args.scene}")
    console.print(f"  Concurrent: {args.concurrent}")
    console.print(f"  Max Reqs  : {args.requests}")
    console.print(f"  Duration  : {args.duration}s")
    console.print()

    try:
        async with aiohttp.ClientSession() as s:
            async with s.get(f"{args.target}/health", timeout=aiohttp.ClientTimeout(total=5)) as r:
                if r.status < 400:
                    console.print(f"[green]Target is reachable[/] (HTTP {r.status})")
                else:
                    console.print(f"[yellow]Target returned HTTP {r.status}[/]")
    except Exception:
        console.print("[yellow]Warning: Could not connect to target. Tests will run but expect connection errors.[/]")

    scenarios_to_run = list(SCENARIOS.keys()) if args.scenario == "all" else [args.scenario]
    results: list[ScenarioResult] = []

    for name in scenarios_to_run:
        fn = SCENARIOS[name]
        try:
            r = await fn(args.target, args.concurrent, args.requests, args.duration)
            results.append(r)
        except Exception as exc:
            console.print(f"[red]Scenario '{name}' failed: {exc}[/]")
            results.append(ScenarioResult(name=name, failed=1, error_breakdown={"exception": 1}))

    print_report(results)

    if args.output:
        data = [r.to_dict() for r in results]
        with open(args.output, "w") as f:
            json.dump(data, f, indent=2)
        console.print(f"\n[green]Results written to {args.output}[/]")

    # Exit 1 if any scenario had errors (but rate-limit is special)
    for r in results:
        if r.name != "rate-limit" and r.error_rate > 0.5:
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
