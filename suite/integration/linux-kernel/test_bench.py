"""Reject partial, corrupted, and failed guest timing reports."""
import unittest

from bench import COUNTS, measurements


class MeasurementsTests(unittest.TestCase):
    def transcript(self):
        lines = ["BENCH_READY 250000000"]
        lines.extend(f"BENCH {name} 1000000 {count}" for name, count in COUNTS.items())
        lines.append("KEEL_BENCH_OK")
        return ("\r\n".join(lines) + "\r\n").encode()

    def test_complete_checked_run(self):
        result = measurements(self.transcript(), 33)
        self.assertEqual(result["guest_boot_ns"], 250000000)
        self.assertEqual(set(result), {"guest_boot_ns", *COUNTS})

    def test_failure_exit_and_failure_marker(self):
        with self.assertRaises(ValueError):
            measurements(self.transcript(), 35)
        with self.assertRaises(ValueError):
            measurements(self.transcript() + b"KEEL_BENCH_FAIL\r\n", 33)

    def test_missing_or_duplicate_workload(self):
        line = f"BENCH getpid 1000000 {COUNTS['getpid']}\r\n".encode()
        with self.assertRaises(ValueError):
            measurements(self.transcript().replace(line, b""), 33)
        with self.assertRaises(ValueError):
            measurements(self.transcript() + line, 33)

    def test_wrong_work_count_or_zero_time(self):
        with self.assertRaises(ValueError):
            measurements(self.transcript().replace(b"5000000\r", b"4999999\r"), 33)
        with self.assertRaises(ValueError):
            measurements(self.transcript().replace(b" 1000000 ", b" 0 "), 33)

    def test_duplicate_startup_or_missing_success(self):
        with self.assertRaises(ValueError):
            measurements(self.transcript() + b"BENCH_READY 250000000\r\n", 33)
        with self.assertRaises(ValueError):
            measurements(self.transcript().replace(b"KEEL_BENCH_OK", b""), 33)


if __name__ == "__main__":
    unittest.main()
