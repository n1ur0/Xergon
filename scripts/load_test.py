#!/usr/bin/env python3
"""Xergon Network Load Test

Usage:
    python load_test.py [--url URL] [--concurrent N] [--requests N] [--duration S]

Examples:
    python load_test.py                                    # defaults
    python load_test.py --concurrent 50 --requests 5000   # custom load
    python load_test.py --duration 30                     # time-based test
    python load_test.py --url http://localhost:9011       # custom relay URL

Reports: p50/p95/p99 latency, throughput (req/s), error rate, connection errors
"""

import asyncio
import aiohttp
import time
import argparse
import json
import statistics
import sys
from dataclasses import dataclass, field
from typing import List, Optional


@dataclass
class RequestResult:
    endpoint: str
    status: int
    latency_ms: float
    error: Optional[str] = None
    success: bool = True


@dataclass
class TestReport:
    endpoint: str
    total_requests: int
    successful: int
    failed: int
    error_rate: float
    throughput_rps: float
    latencies_ms: List[float] = field(default_factory=list)

    @property
    def avg_latency(self) -> float:
        return statistics.mean(self.latencies_ms) if self.latencies_ms else 0.0

    @property
    def p50_latency(self) -> float:
        return statistics.median(self.latencies_ms) if self.latencies_ms else 0.0

    @property
    def p95_latency(self) -> float:
        if not self.latencies_ms:
            return 0.0
        s = sorted(self.latencies_ms)
        idx = int(len(s) * 0.95)
        return s[min(idx, len(s) - 1)]

    @property
    def p99_latency(self) -> float:
        if not self.latencies_ms:
            return 0.0
        s = sorted(self.latencies_ms)
        idx = int(len(s) * 0.99)
        return s[min(idx, len(s) - 1)]

    @property
    def min_latency(self) -> float:
        return min(self.latencies_ms) if self.latencies_ms else 0.0

    @property
    def max_latency(self) -> float:
        return max(self.latencies_ms) if self.latencies_ms else 0.0


async def check_endpoint(session: aiohttp.ClientSession, url: str) -> bool:
    """Check if an endpoint is reachable."""
    try:
        async with session.get(url, timeout=aiohttp.ClientTimeout(total=3)) as resp:
            return resp.status < 500
    except Exception:
        return False


async def make_request(
    session: aiohttp.ClientSession,
    method: str,
    url: str,
    endpoint_name: str,
    headers: Optional[dict] = None,
    json_body: Optional[dict] = None,
) -> RequestResult:
    """Make a single HTTP request and measure latency."""
    start = time.monotonic()
    try:
        async with session.request(
            method, url, headers=headers, json=json_body,
            timeout=aiohttp.ClientTimeout(total=30)
        ) as resp:
            latency_ms = (time.monotonic() - start) * 1000
            try:
                await resp.read()
            except Exception:
                pass
            return RequestResult(
                endpoint=endpoint_name,
                status=resp.status,
                latency_ms=latency_ms,
                success=200 <= resp.status < 300,
                error=None if 200 <= resp.status < 300 else f"HTTP {resp.status}",
            )
    except asyncio.TimeoutError:
        latency_ms = (time.monotonic() - start) * 1000
        return RequestResult(
            endpoint=endpoint_name, status=0, latency_ms=latency_ms,
            success=False, error="timeout",
        )
    except aiohttp.ClientError as e:
        latency_ms = (time.monotonic() - start) * 1000
        return RequestResult(
            endpoint=endpoint_name, status=0, latency_ms=latency_ms,
            success=False, error=str(e),
        )
    except Exception as e:
        latency_ms = (time.monotonic() - start) * 1000
        return RequestResult(
            endpoint=endpoint_name, status=0, latency_ms=latency_ms,
            success=False, error=f"unknown: {e}",
        )


def build_report(endpoint: str, results: List[RequestResult], elapsed: float) -> TestReport:
    """Build a summary report from request results."""
    successful = [r for r in results if r.success]
    failed = [r for r in results if not r.success]
    return TestReport(
        endpoint=endpoint,
        total_requests=len(results),
        successful=len(successful),
        failed=len(failed),
        error_rate=(len(failed) / len(results) * 100) if results else 0.0,
        throughput_rps=len(results) / elapsed if elapsed > 0 else 0.0,
        latencies_ms=[r.latency_ms for r in successful],
    )


def print_report(report: TestReport) -> None:
    """Print a formatted test report."""
    print(f"  Endpoint:      {report.endpoint}")
    print(f"  Total:         {report.total_requests}")
    print(f"  Successful:    {report.successful}")
    print(f"  Failed:        {report.failed}")
    print(f"  Error rate:    {report.error_rate:.1f}%")
    print(f"  Throughput:    {report.throughput_rps:.1f} req/s")
    if report.latencies_ms:
        print(f"  Latency min:   {report.min_latency:.1f} ms")
        print(f"  Latency avg:   {report.avg_latency:.1f} ms")
        print(f"  Latency p50:   {report.p50_latency:.1f} ms")
        print(f"  Latency p95:   {report.p95_latency:.1f} ms")
        print(f"  Latency p99:   {report.p99_latency:.1f} ms")
        print(f"  Latency max:   {report.max_latency:.1f} ms")
    print()


async def run_test(
    session: aiohttp.ClientSession,
    method: str,
    url: str,
    endpoint_name: str,
    concurrent: int,
    total_requests: int,
    duration: Optional[float] = None,
    headers: Optional[dict] = None,
    json_body: Optional[dict] = None,
) -> TestReport:
    """Run a load test against a single endpoint."""
    print(f"  Testing {endpoint_name} ({method} {url})...")

    semaphore = asyncio.Semaphore(concurrent)
    results: List[RequestResult] = []
    completed = 0
    request_count = 0
    stop_time = None
    lock = asyncio.Lock()

    if duration:
        stop_time = time.monotonic() + duration

    async def bounded_request():
        nonlocal completed, request_count
        async with semaphore:
            async with lock:
                if stop_time and time.monotonic() >= stop_time:
                    return
                request_count += 1

            result = await make_request(
                session, method, url, endpoint_name,
                headers=headers, json_body=json_body,
            )
            async with lock:
                results.append(result)
                completed += 1

    start = time.monotonic()

    if duration:
        # Time-based: launch requests until duration expires
        tasks = []
        while time.monotonic() < stop_time:
            task = asyncio.create_task(bounded_request())
            tasks.append(task)
            await asyncio.sleep(0)  # yield control
        await asyncio.gather(*tasks, return_exceptions=True)
    else:
        # Count-based: launch exactly total_requests
        tasks = [asyncio.create_task(bounded_request()) for _ in range(total_requests)]
        await asyncio.gather(*tasks, return_exceptions=True)

    elapsed = time.monotonic() - start
    return build_report(endpoint_name, results, elapsed)


async def main():
    parser = argparse.ArgumentParser(description="Xergon Network Load Test")
    parser.add_argument("--agent-url", default="http://localhost:9010", help="Agent URL")
    parser.add_argument("--relay-url", default="http://localhost:9011", help="Relay URL")
    parser.add_argument("--concurrent", "-c", type=int, default=50, help="Concurrent requests")
    parser.add_argument("--requests", "-n", type=int, default=1000, help="Total requests per endpoint")
    parser.add_argument("--duration", "-d", type=float, default=None, help="Duration in seconds (overrides --requests)")
    parser.add_argument("--pk", default=None, help="Public key for authenticated tests")
    args = parser.parse_args()

    agent_url = args.agent_url.rstrip("/")
    relay_url = args.relay_url.rstrip("/")

    print("=" * 60)
    print("  Xergon Network Load Test")
    print("=" * 60)
    print(f"  Agent URL:  {agent_url}")
    print(f"  Relay URL:  {relay_url}")
    print(f"  Concurrent: {args.concurrent}")
    if args.duration:
        print(f"  Duration:   {args.duration}s")
    else:
        print(f"  Requests:   {args.requests}")
    print()

    connector = aiohttp.TCPConnector(limit=args.concurrent, limit_per_host=args.concurrent)
    async with aiohttp.ClientSession(connector=connector) as session:
        # --- Agent Tests ---
        print("[1/5] Agent Health")
        agent_health_ok = await check_endpoint(session, f"{agent_url}/api/health")
        if agent_health_ok:
            report = await run_test(
                session, "GET", f"{agent_url}/api/health", "Agent Health",
                args.concurrent, args.requests, args.duration,
            )
            print_report(report)
        else:
            print("  SKIPPED - Agent not reachable\n")

        print("[2/5] Agent Metrics")
        agent_metrics_ok = await check_endpoint(session, f"{agent_url}/api/metrics")
        if agent_metrics_ok:
            report = await run_test(
                session, "GET", f"{agent_url}/api/metrics", "Agent Metrics",
                args.concurrent, args.requests, args.duration,
            )
            print_report(report)
        else:
            print("  SKIPPED - Agent not reachable\n")

        # --- Relay Tests ---
        print("[3/5] Relay Health")
        relay_health_ok = await check_endpoint(session, f"{relay_url}/v1/health")
        if relay_health_ok:
            report = await run_test(
                session, "GET", f"{relay_url}/v1/health", "Relay Health",
                args.concurrent, args.requests, args.duration,
            )
            print_report(report)
        else:
            print("  SKIPPED - Relay not reachable\n")

        print("[4/5] Relay Models")
        relay_models_ok = await check_endpoint(session, f"{relay_url}/v1/models")
        if relay_models_ok:
            report = await run_test(
                session, "GET", f"{relay_url}/v1/models", "Relay Models",
                args.concurrent, args.requests, args.duration,
            )
            print_report(report)
        else:
            print("  SKIPPED - Relay not reachable\n")

        # --- Chat Test (requires auth) ---
        print("[5/5] Relay Chat Completions")
        if args.pk and relay_health_ok:
            import hashlib
            import hmac

            timestamp = str(int(time.time()))
            message = f"{timestamp}POST/v1/chat/completions"
            signature = hmac.new(
                args.pk.encode(), message.encode(), hashlib.sha256
            ).hexdigest()

            headers = {
                "Content-Type": "application/json",
                "X-Timestamp": timestamp,
                "X-Signature": signature,
                "X-Public-Key": args.pk,
            }
            body = {
                "model": "llama3.1:8b",
                "messages": [{"role": "user", "content": "Hello"}],
                "max_tokens": 10,
            }
            report = await run_test(
                session, "POST", f"{relay_url}/v1/chat/completions", "Relay Chat",
                args.concurrent, args.requests, args.duration,
                headers=headers, json_body=body,
            )
            print_report(report)
        else:
            if not args.pk:
                print("  SKIPPED - No PK provided (use --pk)\n")
            else:
                print("  SKIPPED - Relay not reachable\n")

    print("=" * 60)
    print("  Load Test Complete")
    print("=" * 60)


if __name__ == "__main__":
    asyncio.run(main())
