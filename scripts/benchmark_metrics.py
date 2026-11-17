"""Retain host conditions and distinguish elapsed latency from child CPU usage."""

import math
import os
import statistics
import time
from typing import TypedDict


class HostState(TypedDict):
    captured_at_unix: float
    logical_cpus: int | None
    available_cpus: int | None
    load_average: list[float] | None
    load_per_cpu: float | None
    telemetry_error: str | None


class Sample(TypedDict):
    elapsed_ms: float
    peak_rss_bytes: int | None
    cpu_ms: float | None
    host_before: HostState
    host_after: HostState


def host_state() -> HostState:
    """Capture contention indicators without treating waiting time as CPU work."""
    available = os.process_cpu_count()
    load, error = None, None
    try:
        load = list(os.getloadavg())
    except (AttributeError, OSError) as failure:
        error = type(failure).__name__
    return {
        "captured_at_unix": time.time(),
        "logical_cpus": os.cpu_count(),
        "available_cpus": available,
        "load_average": load,
        "load_per_cpu": load[0] / available if load and available else None,
        "telemetry_error": error,
    }


def summarise(samples: list[Sample]) -> dict[str, float | int | None]:
    """Use a nearest-rank percentile and preserve missing CPU observations."""
    elapsed = sorted(sample["elapsed_ms"] for sample in samples)
    if not elapsed:
        raise ValueError("At least one measurement is required")
    cpu = [sample["cpu_ms"] for sample in samples if sample["cpu_ms"] is not None]
    return {
        "sample_count": len(elapsed),
        "median_ms": statistics.median(elapsed),
        "minimum_ms": elapsed[0],
        "maximum_ms": elapsed[-1],
        "p95_ms": elapsed[math.ceil(len(elapsed) * 0.95) - 1],
        "median_cpu_ms": statistics.median(cpu) if cpu else None,
        "cpu_sample_count": len(cpu),
    }
