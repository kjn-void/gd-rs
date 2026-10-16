"""Tests for interruption rejection and checkpoint auditing (no native timing)."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("comparison", Path(__file__).with_name("compare.py"))
comparison = importlib.util.module_from_spec(spec)
spec.loader.exec_module(comparison)


class TimingValidationTests(unittest.TestCase):
    def test_fast_copies_and_cpu_bound_operations_are_accepted(self):
        for nanos in [[29_000, 30_000, 31_000], [900_000_000, 1_000_000_000, 1_100_000_000]]:
            self.assertIsNone(comparison.interruption_reason({"nanos": nanos, "off_cpu_seconds": 0.02}))

    def test_suspension_and_short_sample_preemption_are_rejected(self):
        self.assertIsNotNone(comparison.interruption_reason({"nanos": [30_000, 1_000_000_000_000]}))
        self.assertIsNotNone(comparison.interruption_reason({"nanos": [30_000, 20_000_000]}))
        self.assertIsNotNone(comparison.interruption_reason({"nanos": [40_000_000_000] * 5}))

    def test_process_off_cpu_time_is_checked_even_when_samples_are_close(self):
        self.assertIsNotNone(comparison.interruption_reason({"nanos": [30_000] * 5,
                                                             "off_cpu_seconds": 1.5}))

    def test_resume_discards_the_whole_interrupted_round(self):
        def record(round_index, implementation, nanos):
            return dict(rows=20, unique=1, length=128, round=round_index,
                        implementation=implementation, nanos=nanos)
        good = [record(1, "default", [30_000] * 5), record(1, "interned", [50_000] * 5)]
        bad = [record(2, "default", [30_000] * 5),
               record(2, "interned", [50_000, 1_000_000_000_000])]
        report = {"measurements": good + bad, "discarded_rounds": []}
        comparison.discard_interrupted_rounds(report)
        self.assertEqual(report["measurements"], good)
        self.assertEqual(report["discarded_rounds"][0]["measurements"], bad)


if __name__ == "__main__":
    unittest.main()
