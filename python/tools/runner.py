#!/usr/bin/env python3
"""
MEL Scenario Test Runner & Grading Harness.

Usage:
    python3 runner.py --test
    python3 runner.py --scenario scenarios/01_first_boot.yaml --binary build/boot.bin
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

# Ensure project root is in sys.path
REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from python.tools.scenario_eval import (
    GradeReport,
    MelError,
    ScenarioConfigError,
    ScenarioEvaluator,
)


def find_mel_binary() -> Path | None:
    """
    Search for the MEL CLI executable in standard build output directories.

    Order of precedence:
      1. MEL_BIN environment variable.
      2. REPO_ROOT / target / release / mel
      3. REPO_ROOT / target / debug / mel
    """
    if env_bin := os.environ.get("MEL_BIN"):
        p = Path(env_bin).resolve()
        if p.is_file() and os.access(p, os.X_OK):
            return p

    candidates = [
        REPO_ROOT / "target" / "release" / "mel",
        REPO_ROOT / "target" / "debug" / "mel",
    ]
    for candidate in candidates:
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return candidate

    return None


def execute_binary_with_cli(
    mel_bin: str | Path,
    binary_path: str | Path,
    arch: str = "riscv64",
    cycles: int = 100_000,
    *,
    timeout_sec: float = 30.0,
) -> dict[str, Any]:
    """
    Execute raw binary payload using mel CLI and capture execution results.

    Args:
        mel_bin: Path to compiled mel binary.
        binary_path: Path to guest payload binary to execute.
        arch: Target CPU architecture.
        cycles: Maximum cycles to execute.
        timeout_sec: Maximum wall-clock seconds before timing out.

    Returns:
        Dictionary containing stdout, stderr, and returncode.
    """
    cmd = [
        str(mel_bin),
        "run",
        "--raw",
        str(binary_path),
        "--arch",
        arch,
        "--cycles",
        str(cycles),
    ]

    try:
        res = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout_sec,
            check=False,
        )
        return {
            "stdout": res.stdout,
            "stderr": res.stderr,
            "returncode": res.returncode,
            "timed_out": False,
        }
    except subprocess.TimeoutExpired as exc:
        return {
            "stdout": (exc.stdout or "").decode("utf-8", errors="replace") if isinstance(exc.stdout, bytes) else (exc.stdout or ""),
            "stderr": f"Execution timed out after {timeout_sec}s",
            "returncode": -1,
            "timed_out": True,
        }


def run_self_test() -> int:
    """Run automated self-tests of the evaluation harness."""
    print("[*] Running MEL Test Runner Self-Test Suite...")

    sample_scenario = {
        "id": "scenario_mock_01",
        "name": "Mock Test Scenario",
        "description": "Verify harness self-test functionality",
        "architecture": "riscv64",
        "timeout_cycles": 10000,
        "objectives": [
            {
                "type": "register_equals",
                "register": "sp",
                "value": 0x80010000,
                "description": "Stack pointer initialized to 0x80010000",
            },
            {
                "type": "uart_output_contains",
                "expected": "BOOT OK",
                "description": "UART displays BOOT OK",
            },
            {
                "type": "no_traps",
                "description": "Execution without CPU trap",
            },
            {
                "type": "cycles_under",
                "max_cycles": 5000,
                "description": "Cycles under 5000",
            },
        ],
    }

    evaluator = ScenarioEvaluator(scenario_data=sample_scenario)

    # 1. Test passing state
    mock_passing_regs = {"sp": 0x80010000, "pc": 0x80000010}
    mock_passing_mem: dict[int, bytes] = {}
    mock_passing_uart = "System init...\nBOOT OK\nReady."
    report_pass = evaluator.evaluate_state(
        registers=mock_passing_regs,
        memory=mock_passing_mem,
        uart_output=mock_passing_uart,
        cycles=1200,
        trapped=False,
        pc=0x80000010,
    )

    assert report_pass.passed is True, f"Expected passed=True, got {report_pass.passed}"
    assert report_pass.score == 4, f"Expected score=4, got {report_pass.score}"
    assert report_pass.max_score == 4, f"Expected max_score=4, got {report_pass.max_score}"
    assert report_pass.percentage == 100.0
    print("  [+] Passing scenario evaluation test passed.")

    # 2. Test failing state
    mock_failing_regs = {"sp": 0x0}
    mock_failing_uart = "Fatal error"
    report_fail = evaluator.evaluate_state(
        registers=mock_failing_regs,
        memory={},
        uart_output=mock_failing_uart,
        cycles=6000,
        trapped=True,
        pc=0x0,
    )

    assert report_fail.passed is False, f"Expected passed=False, got {report_fail.passed}"
    assert report_fail.score == 0, f"Expected score=0, got {report_fail.score}"
    assert report_fail.percentage == 0.0
    print("  [+] Failing scenario evaluation test passed.")

    # 3. Test JSON serialization and summary
    json_str = report_pass.to_json()
    parsed_json = json.loads(json_str)
    assert parsed_json["scenario_id"] == "scenario_mock_01"
    assert parsed_json["percentage"] == 100.0
    summary = report_pass.summary()
    assert "Result:   PASSED" in summary
    print("  [+] JSON report serialization and summary test passed.")

    print("\n[SUCCESS] All runner harness self-tests passed cleanly!\n")
    return 0


def main(argv: list[str] | None = None) -> int:
    """CLI entry point for scenario runner."""
    parser = argparse.ArgumentParser(
        description="MEL Scenario Test Runner & Grading Harness",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("--test", action="store_true", help="Run automated self-tests")
    parser.add_argument("--scenario", type=Path, help="Path to scenario YAML definition")
    parser.add_argument("--binary", type=Path, help="Path to binary payload to evaluate")
    parser.add_argument("--arch", type=str, default="riscv64", help="Architecture (default: riscv64)")
    parser.add_argument("--cycles", type=int, default=100_000, help="Cycle execution limit")
    parser.add_argument("--json", action="store_true", help="Output grade report in JSON format")

    args = parser.parse_args(argv)

    if args.test:
        return run_self_test()

    if not args.scenario:
        parser.print_help()
        print("\nError: --scenario or --test required", file=sys.stderr)
        return 2

    try:
        evaluator = ScenarioEvaluator(scenario_yaml_path=args.scenario)
    except (FileNotFoundError, ScenarioConfigError) as err:
        print(f"Error loading scenario: {err}", file=sys.stderr)
        return 1

    if not args.json:
        print(f"[*] Loaded Scenario: {evaluator.name} ({evaluator.id})")

    # If binary provided and mel executable found, run it
    mel_bin = find_mel_binary()
    if args.binary and mel_bin:
        exec_res = execute_binary_with_cli(
            mel_bin=mel_bin,
            binary_path=args.binary,
            arch=args.arch,
            cycles=args.cycles,
        )
        report = evaluator.evaluate_state(
            registers={},
            memory={},
            uart_output=exec_res["stdout"],
            cycles=args.cycles,
            trapped=(exec_res["returncode"] != 0),
        )
    else:
        # Mock evaluation when no binary is provided
        report = evaluator.evaluate_state(
            registers={},
            memory={},
            uart_output="",
            cycles=0,
            trapped=False,
        )

    if args.json:
        print(report.to_json())
    else:
        print(f"\n{report.summary()}")

    return 0 if report.passed else 1


if __name__ == "__main__":
    sys.exit(main())
