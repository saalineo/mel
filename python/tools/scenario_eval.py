#!/usr/bin/env python3
"""
MEL Scenario Evaluator and Objective Assertion Engine.

Provides evaluation of machine execution traces against declarative YAML scenario objectives.
"""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Callable, Mapping, Sequence
import yaml


# --- Domain Exceptions (Effective Python Item 43, 87) ---

class MelError(Exception):
    """Base exception for all MEL tooling errors."""


class ScenarioError(MelError):
    """Raised when scenario processing or assertion fails."""


class ScenarioConfigError(ScenarioError):
    """Raised when scenario configuration is missing or malformed."""


# --- Data Structures (Effective Python Items 37, 41) ---

def _format_hex(val: int | None) -> str:
    """Safely format an integer as hex or return 'None'."""
    return f"0x{val:x}" if val is not None else "None"


@dataclass(slots=True)
class ObjectiveResult:
    """Represents the outcome of a single scenario objective assertion."""

    description: str
    passed: bool
    details: str
    expected: Any = None
    actual: Any = None

    def to_dict(self) -> dict[str, Any]:
        """Convert result to a JSON-compatible dictionary."""
        return {
            "description": self.description,
            "passed": self.passed,
            "details": self.details,
            "expected": self.expected,
            "actual": self.actual,
        }

    def __str__(self) -> str:
        tag = "[OK]  " if self.passed else "[FAIL]"
        return f"{tag} {self.description}: {self.details}"


@dataclass(slots=True)
class GradeReport:
    """Aggregated grade report for all objectives within a scenario evaluation."""

    scenario_id: str
    scenario_name: str
    passed: bool
    score: int
    max_score: int
    cycles_elapsed: int
    objectives: list[ObjectiveResult] = field(default_factory=list)

    @property
    def percentage(self) -> float:
        """Percentage of objectives passed."""
        if self.max_score <= 0:
            return 100.0
        return (self.score / self.max_score) * 100.0

    def to_dict(self) -> dict[str, Any]:
        """Convert grade report to dictionary for structured serialization."""
        return {
            "scenario_id": self.scenario_id,
            "scenario_name": self.scenario_name,
            "passed": self.passed,
            "score": self.score,
            "max_score": self.max_score,
            "percentage": self.percentage,
            "cycles_elapsed": self.cycles_elapsed,
            "objectives": [obj.to_dict() for obj in self.objectives],
        }

    def to_json(self, indent: int = 2) -> str:
        """Serialize grade report to JSON."""
        return json.dumps(self.to_dict(), indent=indent)

    def summary(self) -> str:
        """Generate human-readable multi-line summary string."""
        status = "PASSED" if self.passed else "FAILED"
        lines = [
            f"Scenario: {self.scenario_name} ({self.scenario_id})",
            f"Result:   {status}",
            f"Score:    {self.score}/{self.max_score} ({self.percentage:.1f}%)",
            f"Cycles:   {self.cycles_elapsed}",
            "Objectives:",
        ]
        for obj in self.objectives:
            lines.append(f"  {obj}")
        return "\n".join(lines)


# --- Scenario Evaluator Class ---

class ScenarioEvaluator:
    """Evaluates machine state against declarative scenario objectives."""

    def __init__(
        self,
        scenario_yaml_path: str | Path | None = None,
        scenario_data: dict[str, Any] | None = None,
    ) -> None:
        """
        Initialize evaluator from either a YAML file path or pre-loaded dictionary.

        Raises:
            ScenarioConfigError: If neither or both parameters are missing/invalid.
            FileNotFoundError: If the YAML file path does not exist.
        """
        if scenario_yaml_path:
            path = Path(scenario_yaml_path)
            if not path.is_file():
                raise FileNotFoundError(f"Scenario file not found at: {path}")
            with path.open("r", encoding="utf-8") as f:
                loaded = yaml.safe_load(f)
                if not isinstance(loaded, dict):
                    raise ScenarioConfigError(f"Expected mapping in YAML file '{path}', got {type(loaded).__name__}")
                self.data = loaded
        elif scenario_data is not None:
            if not isinstance(scenario_data, dict):
                raise ScenarioConfigError("scenario_data must be a dict")
            self.data = scenario_data
        else:
            raise ScenarioConfigError("Either scenario_yaml_path or scenario_data must be provided")

    @property
    def id(self) -> str:
        """Scenario unique identifier."""
        return str(self.data.get("id", "scenario_unknown"))

    @property
    def name(self) -> str:
        """Scenario descriptive name."""
        return str(self.data.get("name", "Unnamed Scenario"))

    @property
    def timeout_cycles(self) -> int:
        """Maximum execution cycles allowed for the scenario."""
        return int(self.data.get("timeout_cycles", 1_000_000))

    @property
    def architecture(self) -> str:
        """Target architecture string identifier."""
        return str(self.data.get("architecture", "riscv64"))

    # --- Objective Handlers (Effective Python Item 38: Dispatchers) ---

    @staticmethod
    def _eval_register_equals(
        obj: dict[str, Any],
        registers: Mapping[str, int],
        desc: str,
    ) -> ObjectiveResult:
        reg = obj.get("register", "")
        exp_val = obj.get("value")
        actual_val = registers.get(reg)
        passed = actual_val == exp_val
        detail = f"Register {reg}: expected {_format_hex(exp_val)}, got {_format_hex(actual_val)}"
        return ObjectiveResult(desc, passed, detail, exp_val, actual_val)

    @staticmethod
    def _eval_register_in_range(
        obj: dict[str, Any],
        registers: Mapping[str, int],
        desc: str,
    ) -> ObjectiveResult:
        reg = obj.get("register", "")
        min_val = obj.get("min", 0)
        max_val = obj.get("max", 0)
        actual_val = registers.get(reg)
        passed = actual_val is not None and (min_val <= actual_val <= max_val)
        detail = (
            f"Register {reg}: expected in [{_format_hex(min_val)}, {_format_hex(max_val)}], "
            f"got {_format_hex(actual_val)}"
        )
        return ObjectiveResult(desc, passed, detail, [min_val, max_val], actual_val)

    @staticmethod
    def _eval_memory_contains(
        obj: dict[str, Any],
        memory: Mapping[int, bytes],
        desc: str,
    ) -> ObjectiveResult:
        addr = obj.get("address", 0)
        exp_bytes = bytes(obj.get("expected_bytes", []))
        actual_bytes = memory.get(addr, b"")[: len(exp_bytes)]
        passed = actual_bytes == exp_bytes
        detail = f"Memory at {_format_hex(addr)}: expected {exp_bytes.hex()}, got {actual_bytes.hex()}"
        return ObjectiveResult(desc, passed, detail, exp_bytes.hex(), actual_bytes.hex())

    @staticmethod
    def _eval_uart_output_contains(
        obj: dict[str, Any],
        uart_output: str,
        desc: str,
    ) -> ObjectiveResult:
        exp_sub = str(obj.get("expected", ""))
        passed = exp_sub in uart_output
        detail = f"UART contains '{exp_sub}' (Actual: '{uart_output}')"
        return ObjectiveResult(desc, passed, detail, exp_sub, uart_output)

    @staticmethod
    def _eval_uart_output_exact(
        obj: dict[str, Any],
        uart_output: str,
        desc: str,
    ) -> ObjectiveResult:
        exp_exact = str(obj.get("expected", ""))
        passed = uart_output == exp_exact
        detail = f"UART exact match '{exp_exact}' (Actual: '{uart_output}')"
        return ObjectiveResult(desc, passed, detail, exp_exact, uart_output)

    @staticmethod
    def _eval_cycles_under(
        obj: dict[str, Any],
        cycles: int,
        desc: str,
    ) -> ObjectiveResult:
        max_c = int(obj.get("max_cycles", 0))
        passed = cycles <= max_c
        detail = f"Executed {cycles} cycles (Limit: {max_c})"
        return ObjectiveResult(desc, passed, detail, max_c, cycles)

    @staticmethod
    def _eval_no_traps(
        _obj: dict[str, Any],
        trapped: bool,
        desc: str,
    ) -> ObjectiveResult:
        passed = not trapped
        detail = "No unhandled traps" if not trapped else "CPU trap occurred"
        return ObjectiveResult(desc, passed, detail, False, trapped)

    @staticmethod
    def _eval_program_counter_equals(
        obj: dict[str, Any],
        pc: int | None,
        desc: str,
    ) -> ObjectiveResult:
        target_pc = obj.get("address")
        passed = pc == target_pc
        detail = f"PC halted at {_format_hex(target_pc)} (Actual: {_format_hex(pc)})"
        return ObjectiveResult(desc, passed, detail, target_pc, pc)

    # --- Main Evaluator Entry Point (Effective Python Item 25: Keyword-Only Args) ---

    def evaluate_state(
        self,
        registers: Mapping[str, int],
        memory: Mapping[int, bytes],
        uart_output: str,
        cycles: int,
        *,
        trapped: bool = False,
        pc: int | None = None,
    ) -> GradeReport:
        """
        Evaluate simulation state against all objectives defined in the scenario.

        Args:
            registers: Mapping of register names to register integer values.
            memory: Mapping of physical memory addresses to byte arrays.
            uart_output: String accumulated from UART serial output.
            cycles: Total cycles elapsed during execution.
            trapped: Boolean indicating whether CPU raised an unhandled hardware trap.
            pc: Current Program Counter value at termination.

        Returns:
            A GradeReport containing individual objective results and overall scoring.
        """
        results: list[ObjectiveResult] = []
        objectives: Sequence[dict[str, Any]] = self.data.get("objectives", [])
        passed_count = 0

        for obj in objectives:
            obj_type = obj.get("type")
            desc = obj.get("description", f"Objective: {obj_type}")

            match obj_type:
                case "register_equals":
                    res = self._eval_register_equals(obj, registers, desc)
                case "register_in_range":
                    res = self._eval_register_in_range(obj, registers, desc)
                case "memory_contains":
                    res = self._eval_memory_contains(obj, memory, desc)
                case "uart_output_contains":
                    res = self._eval_uart_output_contains(obj, uart_output, desc)
                case "uart_output_exact":
                    res = self._eval_uart_output_exact(obj, uart_output, desc)
                case "cycles_under":
                    res = self._eval_cycles_under(obj, cycles, desc)
                case "no_traps":
                    res = self._eval_no_traps(obj, trapped, desc)
                case "program_counter_equals":
                    res = self._eval_program_counter_equals(obj, pc, desc)
                case _:
                    res = ObjectiveResult(desc, False, f"Unknown objective type: '{obj_type}'")

            results.append(res)
            if res.passed:
                passed_count += 1

        total = len(objectives)
        all_passed = (passed_count == total) if total > 0 else True

        return GradeReport(
            scenario_id=self.id,
            scenario_name=self.name,
            passed=all_passed,
            score=passed_count,
            max_score=total,
            cycles_elapsed=cycles,
            objectives=results,
        )
