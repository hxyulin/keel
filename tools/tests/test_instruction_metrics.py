import copy
import importlib.util
import json
import os
import tempfile
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("metrics", Path(__file__).resolve().parents[1] / "instruction_metrics.py")
metrics = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(metrics)


def mca_report():
    # Schema verified against LLVM PipelinePrinter, InstructionInfoView and
    # SummaryView, plus llvm/test/tools/llvm-mca/JSON/X86/views.s. Synthetic
    # values below test parsing, and are not a real CPU performance dataset.
    return {
        "TargetInfo": {"CPUName": "test-cpu"},
        "SimulationParameters": {"-mcpu": "test-cpu"},
        "CodeRegions": [{
            "Instructions": ["add x0, x0, x1"],
            "InstructionInfoView": {"InstructionList": [{
                "Instruction": 0, "Latency": 2, "NumMicroOpcodes": 1,
                "RThroughput": 0.5, "mayLoad": False, "mayStore": False,
                "hasUnmodeledSideEffects": False,
            }]},
            "SummaryView": {"Iterations": 100, "TotalCycles": 203,
                            "BlockRThroughput": 0.5},
        }],
    }


class MetricsTests(unittest.TestCase):
    def test_catalog_covers_every_spec_encoding_without_inventing_timings(self):
        catalog = metrics.catalog()
        isa = json.loads(metrics.ISA.read_text())
        self.assertEqual([r["encoding"] for r in catalog["encodings"]],
                         [r["name"] for r in isa["instructions"]])
        self.assertEqual(catalog["encoding_count"], len(isa["instructions"]))
        self.assertEqual(catalog["timing_coverage"], 0)
        for row in catalog["encodings"]:
            self.assertEqual(row["bytes"], 4)
            self.assertIsNone(row["latency_cycles"])
            self.assertEqual(row["base"] & row["mask"], row["base"])

    def test_cpu_identity_and_complete_instruction_coverage_are_required(self):
        for cpu, count in [("wrong-cpu", 1), ("test-cpu", 2)]:
            with self.assertRaises(ValueError):
                metrics.normalize_mca(mca_report(), cpu, count)
        raw = mca_report()
        raw["CodeRegions"][0]["InstructionInfoView"]["InstructionList"][0]["Instruction"] = 9
        with self.assertRaises(ValueError):
            metrics.normalize_mca(raw, "test-cpu", 1)

    def test_estimates_use_simulated_dependencies_not_summed_latency(self):
        report = metrics.normalize_mca(mca_report(), "test-cpu", 1)
        self.assertEqual(report["estimated_cycles_per_iteration"], 2.03)
        self.assertEqual(report["instructions"][0]["latency_cycles"], 2)
        self.assertEqual(report["instructions"][0]["reciprocal_throughput_cycles"], 0.5)
        self.assertTrue(report["complete"])

    def test_missing_model_and_unmodeled_effects_do_not_produce_speed_scores(self):
        for key, value in [("RThroughput", 0), ("hasUnmodeledSideEffects", True)]:
            raw = mca_report()
            raw["CodeRegions"][0]["InstructionInfoView"]["InstructionList"][0][key] = value
            report = metrics.normalize_mca(raw, "test-cpu", 1)
            self.assertFalse(report["complete"])
            self.assertIsNone(report["estimated_cycles_per_iteration"])
        for invalid in [float("nan"), float("inf"), -1, True]:
            raw = mca_report()
            raw["CodeRegions"][0]["SummaryView"]["TotalCycles"] = invalid
            with self.assertRaises(ValueError):
                metrics.normalize_mca(raw, "test-cpu", 1)

    def test_size_cost_works_without_a_cpu_and_speed_refuses_missing_data(self):
        before = {"schema_version": 1, "isa_sha256": "isa", "assembler_sha256": "asm",
                  "exact": {"target": "aarch64-apple-darwin", "text_bytes": 12},
                  "performance": None}
        after = copy.deepcopy(before)
        after["exact"]["text_bytes"] = 8
        self.assertEqual(metrics.compare(before, after, "size")["delta"], -4)
        self.assertEqual(metrics.compare(before, after, "size")["preferred"], "after")
        with self.assertRaises(ValueError):
            metrics.compare(before, after, "speed")
        before["performance"] = metrics.normalize_mca(mca_report(), "test-cpu", 1)
        before["performance"]["tool_sha256"] = "tool"
        after["performance"] = copy.deepcopy(before["performance"])
        after["performance"]["estimated_cycles_per_iteration"] = 1.5
        self.assertEqual(metrics.compare(before, after, "speed")["preferred"], "after")
        after["performance"]["cpu"] = "different-cpu"
        with self.assertRaises(ValueError):
            metrics.compare(before, after, "speed")


@unittest.skipUnless(os.environ.get("KEEL_METRICS_BINARY"), "set KEEL_METRICS_BINARY for CLI integration")
class IntegrationTests(unittest.TestCase):
    def test_exact_reports_and_optional_real_cpu_model(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "sample.s"
            source.write_text("add x0, x0, x1\n")
            keel = Path(os.environ["KEEL_METRICS_BINARY"]).resolve()
            report = metrics.analyze(source, keel)
            self.assertEqual(report["exact"]["instruction_bytes"], 4)
            self.assertEqual(report["exact"]["instructions"][0]["encoding_candidates"], ["ADD_64_addsub_shift"])
            self.assertIsNone(report["performance"])
            if os.environ.get("LLVM_MCA"):
                report = metrics.analyze(source, keel, "apple-m5", os.environ["LLVM_MCA"])
                self.assertTrue(report["performance"]["complete"])
                self.assertGreater(report["performance"]["estimated_cycles_per_iteration"], 0)
                with self.assertRaises(ValueError):
                    metrics.analyze(source, keel, "invalid-keel-cpu", os.environ["LLVM_MCA"])


class SampleTests(unittest.TestCase):
    def test_samples_preserve_fixed_bits_and_vector_aliases(self):
        isa = json.loads(metrics.ISA.read_text())
        for instruction, row in zip(isa["instructions"], metrics.catalog()["encodings"]):
            self.assertEqual(metrics.representative_word(instruction) & row["mask"], row["base"])
        self.assertEqual(metrics.canonical_syntax("cnt.8b v0, v1"), "cnt v0.8b, v1.8b")
        self.assertEqual(metrics.canonical_syntax("addv.8b b0, v1"), "addv b0, v1.8b")


if __name__ == "__main__":
    unittest.main()
