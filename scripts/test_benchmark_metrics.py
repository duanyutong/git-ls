#!/usr/bin/env -S uv run
"""Prevent misleading benchmark summaries when host telemetry is incomplete."""

import unittest
import unittest.mock

import benchmark_metrics


class MetricsTests(unittest.TestCase):
    def test_unavailable_load_is_explicit(self) -> None:
        with unittest.mock.patch.object(benchmark_metrics.os, "getloadavg", side_effect=OSError):
            state = benchmark_metrics.host_state()
        self.assertIsNone(state["load_average"])
        self.assertIsNone(state["load_per_cpu"])
        self.assertEqual(state["telemetry_error"], "OSError")

    def test_normalises_by_available_processors(self) -> None:
        with (
            unittest.mock.patch.object(benchmark_metrics.os, "getloadavg", return_value=(8, 4, 2)),
            unittest.mock.patch.object(benchmark_metrics.os, "process_cpu_count", return_value=4),
        ):
            self.assertEqual(benchmark_metrics.host_state()["load_per_cpu"], 2)

    def test_percentile_and_cpu_use_observed_samples(self) -> None:
        host = benchmark_metrics.host_state()
        samples: list[benchmark_metrics.Sample] = [
            {
                "elapsed_ms": float(index),
                "cpu_ms": 1.0 if index % 2 else None,
                "peak_rss_bytes": None,
                "host_before": host,
                "host_after": host,
            }
            for index in range(1, 21)
        ]
        summary = benchmark_metrics.summarise(samples)
        self.assertEqual(summary["median_ms"], 10.5)
        self.assertEqual(summary["p95_ms"], 19)
        self.assertEqual(summary["cpu_sample_count"], 10)
        self.assertEqual(summary["median_cpu_ms"], 1)
        for sample in samples:
            sample["cpu_ms"] = None
        self.assertIsNone(benchmark_metrics.summarise(samples)["median_cpu_ms"])
        with self.assertRaisesRegex(ValueError, "At least one"):
            benchmark_metrics.summarise([])


if __name__ == "__main__":
    unittest.main()
