#!/usr/bin/env -S uv run
"""Verify that timed lifecycle fixtures preserve their intended comparison shape."""

import importlib.util
import pathlib
import sys
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "benchmark_diff_stat", pathlib.Path(__file__).with_name("benchmark-diff-stat.py")
)
assert SPEC is not None and SPEC.loader is not None
benchmark = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = benchmark
SPEC.loader.exec_module(benchmark)


class LifecycleFixtures(unittest.TestCase):
    def test_large_edit_changes_one_path_without_losing_the_large_layer(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            case = benchmark.Case("edit", 1, "large_edit", paths=8)
            repo = benchmark.fixture(pathlib.Path(directory), case)
            previous = benchmark.git(repo, "rev-parse", "topic-000").decode().strip()
            benchmark.prepare_sample(pathlib.Path("unused"), repo, case, "gix", [], 7)
            changed = benchmark.git(repo, "diff", "--name-only", previous, "topic-000")
            self.assertEqual(changed, b"directory-0000/data\n")
            layer = benchmark.git(repo, "diff", "--name-only", "main", "topic-000")
            self.assertEqual(len(layer.splitlines()), 8)
            self.assertEqual(benchmark.git(repo, "rev-list", "--count", "main..topic-000"), b"1\n")

    def test_stack_edit_preserves_lower_endpoints_and_the_large_top_layer(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            case = benchmark.Case("stack", 3, "stack_edit", paths=8)
            repo = benchmark.fixture(pathlib.Path(directory), case)
            lower = benchmark.git(repo, "rev-parse", "topic-001")
            previous = benchmark.git(repo, "rev-parse", "topic-002").decode().strip()
            benchmark.prepare_sample(pathlib.Path("unused"), repo, case, "gix", [], 3)
            self.assertEqual(benchmark.git(repo, "rev-parse", "topic-001"), lower)
            changed = benchmark.git(repo, "diff", "--name-only", previous, "topic-002")
            self.assertEqual(len(changed.splitlines()), 1)
            layer = benchmark.git(repo, "diff", "--name-only", "topic-001", "topic-002")
            self.assertEqual(len(layer.splitlines()), 8)
            self.assertEqual(benchmark.git(repo, "rev-list", "--count", "main..topic-002"), b"3\n")

    def test_irrelevant_configuration_events_modify_the_named_effective_key(self) -> None:
        for shape, key in [
            ("branch_config", "branch.topic-000.description"),
            ("fetch_config", "fetch.prune"),
        ]:
            with self.subTest(shape=shape), tempfile.TemporaryDirectory() as directory:
                case = benchmark.Case("config", 3, shape)
                repo = benchmark.fixture(pathlib.Path(directory), case)
                refs = benchmark.git(repo, "show-ref")
                benchmark.prepare_sample(pathlib.Path("unused"), repo, case, "gix", [], 3)
                self.assertTrue(benchmark.git(repo, "config", "--get", key).strip())
                self.assertEqual(benchmark.git(repo, "show-ref"), refs)


if __name__ == "__main__":
    unittest.main()
