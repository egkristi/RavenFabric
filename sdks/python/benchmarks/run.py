"""Agent framework benchmark suite for RavenFabric.

Measures:
- Policy check latency (query_policy round-trip)
- Command execution latency (exec round-trip)
- Throughput (sequential commands/sec)
- Framework adapter overhead (adapter call vs raw client call)

Usage:
    python -m benchmarks.run --server rf-mcp-server --policy policy.yaml --iterations 100

Without a running server, uses a mock transport for relative comparison.
"""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys
import time
from dataclasses import asdict, dataclass, field
from typing import Any
from unittest.mock import MagicMock

from ravenfabric.client import McpClient
from ravenfabric.integrations.anthropic import AnthropicAdapter
from ravenfabric.integrations.autogen import AutoGenExecutor
from ravenfabric.integrations.crewai import CrewAITool
from ravenfabric.integrations.langchain import LangChainTool
from ravenfabric.integrations.openai import OpenAIAdapter
from ravenfabric.protocol import JsonRpcResponse
from ravenfabric.transport import StdioTransport


@dataclass
class BenchmarkResult:
    """Result of a single benchmark run."""

    name: str
    framework: str
    iterations: int
    mean_ms: float
    median_ms: float
    p95_ms: float
    p99_ms: float
    min_ms: float
    max_ms: float
    throughput_ops: float
    timings_ms: list[float] = field(default_factory=list, repr=False)


@dataclass
class BenchmarkReport:
    """Complete benchmark report."""

    timestamp: str
    system_info: dict[str, str]
    results: list[BenchmarkResult]

    def to_json(self) -> str:
        return json.dumps(asdict(self), indent=2)

    def print_table(self) -> None:
        """Print results as a formatted table."""
        print(f"\n{'='*80}")
        print("RavenFabric Agent Framework Benchmark Results")
        print(f"{'='*80}")
        print(f"{'Framework':<15} {'Benchmark':<25} {'Mean':>8} {'P95':>8} {'P99':>8} {'Ops/s':>10}")
        print(f"{'-'*80}")
        for r in self.results:
            print(
                f"{r.framework:<15} {r.name:<25} "
                f"{r.mean_ms:>7.2f}ms {r.p95_ms:>7.2f}ms {r.p99_ms:>7.2f}ms "
                f"{r.throughput_ops:>9.1f}"
            )
        print(f"{'='*80}\n")


class MockTransport:
    """Mock transport for benchmarking without a real server.

    Returns realistic-looking responses for exec and query_policy.
    """

    def __init__(self) -> None:
        self._request_id = 0

    def start_sync(self) -> None:
        pass

    def send_sync(self, method: str, params: dict[str, Any]) -> JsonRpcResponse:
        self._request_id += 1

        if method == "initialize":
            return JsonRpcResponse(
                id=self._request_id,
                result={"protocolVersion": "2024-11-05", "capabilities": {}, "serverInfo": {"name": "mock"}},
            )
        elif method == "notifications/initialized":
            return JsonRpcResponse(id=self._request_id, result={})
        elif method == "tools/call":
            tool_name = params.get("name", "")
            if tool_name == "exec":
                return JsonRpcResponse(
                    id=self._request_id,
                    result={"content": [{"type": "text", "text": json.dumps({
                        "output": "benchmark output\n",
                        "exit_code": 0,
                        "duration_ms": 1.5,
                    })}]},
                )
            elif tool_name == "query_policy":
                return JsonRpcResponse(
                    id=self._request_id,
                    result={"content": [{"type": "text", "text": json.dumps({
                        "allowed": True,
                        "reason": "matches allow pattern",
                        "matched_rule": "^echo.*",
                    })}]},
                )
        return JsonRpcResponse(id=self._request_id, result={"content": []})


def _percentile(data: list[float], p: float) -> float:
    """Calculate percentile of sorted data."""
    sorted_data = sorted(data)
    idx = (len(sorted_data) - 1) * p / 100.0
    lower = int(idx)
    upper = lower + 1
    if upper >= len(sorted_data):
        return sorted_data[-1]
    weight = idx - lower
    return sorted_data[lower] * (1 - weight) + sorted_data[upper] * weight


def benchmark_raw_client(client: McpClient, iterations: int) -> list[BenchmarkResult]:
    """Benchmark raw client operations."""
    results: list[BenchmarkResult] = []

    # Benchmark: exec
    timings: list[float] = []
    for _ in range(iterations):
        start = time.perf_counter()
        client.exec_sync("echo hello")
        elapsed = (time.perf_counter() - start) * 1000
        timings.append(elapsed)

    results.append(_make_result("exec", "raw_client", iterations, timings))

    # Benchmark: query_policy
    timings = []
    for _ in range(iterations):
        start = time.perf_counter()
        client.query_policy_sync("echo hello")
        elapsed = (time.perf_counter() - start) * 1000
        timings.append(elapsed)

    results.append(_make_result("query_policy", "raw_client", iterations, timings))

    return results


def benchmark_langchain(transport: Any, iterations: int) -> list[BenchmarkResult]:
    """Benchmark LangChain tool adapter."""
    tool = LangChainTool(transport)
    tool._initialized = True
    tool._client._initialized = True

    timings: list[float] = []
    for _ in range(iterations):
        start = time.perf_counter()
        tool.run("echo hello")
        elapsed = (time.perf_counter() - start) * 1000
        timings.append(elapsed)

    return [_make_result("exec", "langchain", iterations, timings)]


def benchmark_crewai(transport: Any, iterations: int) -> list[BenchmarkResult]:
    """Benchmark CrewAI tool adapter."""
    tool = CrewAITool(transport)
    tool._initialized = True
    tool._client._initialized = True

    timings: list[float] = []
    for _ in range(iterations):
        start = time.perf_counter()
        tool.run("echo hello")
        elapsed = (time.perf_counter() - start) * 1000
        timings.append(elapsed)

    return [_make_result("exec", "crewai", iterations, timings)]


def benchmark_openai(transport: Any, iterations: int) -> list[BenchmarkResult]:
    """Benchmark OpenAI adapter."""
    adapter = OpenAIAdapter(transport)
    adapter._initialized = True
    adapter._client._initialized = True

    timings: list[float] = []
    for _ in range(iterations):
        start = time.perf_counter()
        adapter.handle_function_call("ravenfabric_exec", {"command": "echo hello"})
        elapsed = (time.perf_counter() - start) * 1000
        timings.append(elapsed)

    return [_make_result("exec", "openai", iterations, timings)]


def benchmark_anthropic(transport: Any, iterations: int) -> list[BenchmarkResult]:
    """Benchmark Anthropic adapter."""
    adapter = AnthropicAdapter(transport)
    adapter._initialized = True
    adapter._client._initialized = True

    timings: list[float] = []
    for _ in range(iterations):
        start = time.perf_counter()
        adapter.handle_tool_use("ravenfabric_exec", {"command": "echo hello"})
        elapsed = (time.perf_counter() - start) * 1000
        timings.append(elapsed)

    return [_make_result("exec", "anthropic", iterations, timings)]


def benchmark_autogen(transport: Any, iterations: int) -> list[BenchmarkResult]:
    """Benchmark AutoGen executor."""
    executor = AutoGenExecutor(transport)
    executor._initialized = True
    executor._client._initialized = True

    timings: list[float] = []
    for _ in range(iterations):
        start = time.perf_counter()
        executor.execute_code_blocks([("bash", "echo hello")])
        elapsed = (time.perf_counter() - start) * 1000
        timings.append(elapsed)

    return [_make_result("exec", "autogen", iterations, timings)]


def _make_result(
    name: str, framework: str, iterations: int, timings: list[float]
) -> BenchmarkResult:
    """Create a BenchmarkResult from raw timings."""
    return BenchmarkResult(
        name=name,
        framework=framework,
        iterations=iterations,
        mean_ms=statistics.mean(timings),
        median_ms=statistics.median(timings),
        p95_ms=_percentile(timings, 95),
        p99_ms=_percentile(timings, 99),
        min_ms=min(timings),
        max_ms=max(timings),
        throughput_ops=1000.0 / statistics.mean(timings) if statistics.mean(timings) > 0 else 0,
        timings_ms=timings,
    )


def run_benchmarks(iterations: int = 100, use_mock: bool = True) -> BenchmarkReport:
    """Run the complete benchmark suite.

    Args:
        iterations: Number of iterations per benchmark.
        use_mock: Use mock transport (True) or real server (False).

    Returns:
        Complete benchmark report.
    """
    import platform
    from datetime import datetime, timezone

    transport: Any
    if use_mock:
        transport = MockTransport()
    else:
        raise NotImplementedError("Real server benchmarks require a running rf-mcp-server")

    # Set up client
    client = McpClient(transport)
    client.initialize_sync()

    all_results: list[BenchmarkResult] = []

    # Run all benchmarks
    all_results.extend(benchmark_raw_client(client, iterations))
    all_results.extend(benchmark_langchain(transport, iterations))
    all_results.extend(benchmark_crewai(transport, iterations))
    all_results.extend(benchmark_openai(transport, iterations))
    all_results.extend(benchmark_anthropic(transport, iterations))
    all_results.extend(benchmark_autogen(transport, iterations))

    report = BenchmarkReport(
        timestamp=datetime.now(timezone.utc).isoformat(),
        system_info={
            "python": platform.python_version(),
            "os": platform.system(),
            "arch": platform.machine(),
            "node": platform.node(),
        },
        results=all_results,
    )

    return report


def main() -> None:
    """CLI entry point for benchmarks."""
    parser = argparse.ArgumentParser(description="RavenFabric Agent Framework Benchmarks")
    parser.add_argument("--iterations", type=int, default=100, help="Iterations per benchmark")
    parser.add_argument("--json", action="store_true", help="Output JSON instead of table")
    args = parser.parse_args()

    report = run_benchmarks(iterations=args.iterations, use_mock=True)

    if args.json:
        print(report.to_json())
    else:
        report.print_table()


if __name__ == "__main__":
    main()
