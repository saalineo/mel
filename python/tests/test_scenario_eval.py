"""Unit tests for MEL Python Scenario Evaluator and Runner Harness."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from python.tools.scenario_eval import (
    GradeReport,
    ObjectiveResult,
    ScenarioConfigError,
    ScenarioEvaluator,
)
from python.tools.runner import run_self_test, find_mel_binary


class TestScenarioEvaluator(unittest.TestCase):
    """Test suite for ScenarioEvaluator and GradeReport."""

    def test_init_missing_arguments_raises_error(self) -> None:
        """Verifies initializing without path or dict raises ScenarioConfigError."""
        with self.assertRaises(ScenarioConfigError):
            ScenarioEvaluator()

    def test_init_invalid_file_raises_error(self) -> None:
        """Verifies nonexistent file raises FileNotFoundError."""
        with self.assertRaises(FileNotFoundError):
            ScenarioEvaluator(scenario_yaml_path="/nonexistent/scenario.yaml")

    def test_init_from_yaml_file(self) -> None:
        """Verifies loading from a valid temporary YAML file."""
        content = """
id: temp_test_01
name: Temp Scenario
architecture: riscv64
timeout_cycles: 50000
objectives:
  - type: register_equals
    register: a0
    value: 42
    description: a0 equals 42
"""
        with tempfile.NamedTemporaryFile("w", suffix=".yaml", delete=False) as f:
            f.write(content)
            temp_path = Path(f.name)

        try:
            evaluator = ScenarioEvaluator(scenario_yaml_path=temp_path)
            self.assertEqual(evaluator.id, "temp_test_01")
            self.assertEqual(evaluator.name, "Temp Scenario")
            self.assertEqual(evaluator.architecture, "riscv64")
            self.assertEqual(evaluator.timeout_cycles, 50000)

            # Test evaluation
            report = evaluator.evaluate_state(
                registers={"a0": 42},
                memory={},
                uart_output="",
                cycles=100,
            )
            self.assertTrue(report.passed)
            self.assertEqual(report.score, 1)
            self.assertEqual(report.max_score, 1)
        finally:
            if temp_path.exists():
                temp_path.unlink()

    def test_all_objective_types(self) -> None:
        """Tests assertion logic for all 8 supported scenario objective types."""
        scenario_data = {
            "id": "full_suite",
            "name": "Full Objective Suite",
            "objectives": [
                {"type": "register_equals", "register": "x1", "value": 100, "description": "reg equals"},
                {"type": "register_in_range", "register": "x2", "min": 10, "max": 20, "description": "reg range"},
                {"type": "memory_contains", "address": 0x1000, "expected_bytes": [0xDE, 0xAD], "description": "mem bytes"},
                {"type": "uart_output_contains", "expected": "PASS", "description": "uart substring"},
                {"type": "uart_output_exact", "expected": "PASS\n", "description": "uart exact"},
                {"type": "cycles_under", "max_cycles": 1000, "description": "cycles"},
                {"type": "no_traps", "description": "traps"},
                {"type": "program_counter_equals", "address": 0x80000020, "description": "pc check"},
            ],
        }
        evaluator = ScenarioEvaluator(scenario_data=scenario_data)

        # 1. All pass
        report = evaluator.evaluate_state(
            registers={"x1": 100, "x2": 15},
            memory={0x1000: b"\xDE\xAD\xBE\xEF"},
            uart_output="PASS\n",
            cycles=500,
            trapped=False,
            pc=0x80000020,
        )
        self.assertTrue(report.passed)
        self.assertEqual(report.score, 8)
        self.assertEqual(report.max_score, 8)
        self.assertEqual(report.percentage, 100.0)

        # 2. All fail
        report_fail = evaluator.evaluate_state(
            registers={"x1": 99, "x2": 25},
            memory={0x1000: b"\x00\x00"},
            uart_output="FAIL",
            cycles=2000,
            trapped=True,
            pc=0x0,
        )
        self.assertFalse(report_fail.passed)
        self.assertEqual(report_fail.score, 0)
        self.assertEqual(report_fail.max_score, 8)
        self.assertEqual(report_fail.percentage, 0.0)

    def test_unknown_objective_type(self) -> None:
        """Verifies unknown objective type fails gracefully."""
        scenario_data = {
            "id": "unknown_test",
            "name": "Unknown Type Test",
            "objectives": [
                {"type": "nonexistent_type", "description": "unknown"},
            ],
        }
        evaluator = ScenarioEvaluator(scenario_data=scenario_data)
        report = evaluator.evaluate_state(
            registers={},
            memory={},
            uart_output="",
            cycles=0,
        )
        self.assertFalse(report.passed)
        self.assertEqual(report.score, 0)
        self.assertIn("Unknown objective type", report.objectives[0].details)

    def test_grade_report_serialization(self) -> None:
        """Verifies to_dict, to_json, and summary on GradeReport."""
        obj = ObjectiveResult(
            description="Sample",
            passed=True,
            details="All good",
            expected=1,
            actual=1,
        )
        report = GradeReport(
            scenario_id="s1",
            scenario_name="Scenario 1",
            passed=True,
            score=1,
            max_score=1,
            cycles_elapsed=42,
            objectives=[obj],
        )

        d = report.to_dict()
        self.assertEqual(d["scenario_id"], "s1")
        self.assertEqual(d["percentage"], 100.0)

        j = report.to_json()
        parsed = json.loads(j)
        self.assertEqual(parsed["cycles_elapsed"], 42)

        summary = report.summary()
        self.assertIn("PASSED", summary)
        self.assertIn("[OK]   Sample: All good", summary)


class TestRunner(unittest.TestCase):
    """Test suite for runner module helpers."""

    def test_runner_self_test(self) -> None:
        """Verifies the runner self-test execution returns 0."""
        res = run_self_test()
        self.assertEqual(res, 0)

    def test_find_mel_binary_non_crashing(self) -> None:
        """Verifies binary discovery runs without exceptions."""
        bin_path = find_mel_binary()
        if bin_path is not None:
            self.assertTrue(bin_path.is_file())


if __name__ == "__main__":
    unittest.main()
