import subprocess
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[1]
GITSAVE_BIN = REPO_ROOT / "target" / "release" / "gitsave"


def require_binary():
    if not GITSAVE_BIN.exists():
        pytest.skip("gitsave binary not found at target/release/gitsave")


def run_cli(args, workdir: Path) -> str:
    result = subprocess.run(
        [str(GITSAVE_BIN), *args],
        cwd=workdir,
        text=True,
        capture_output=True,
    )
    if result.returncode != 0:
        raise AssertionError(
            f"Command {' '.join(args)} failed with code {result.returncode}\nSTDOUT:\n{result.stdout}\nSTDERR:\n{result.stderr}"
        )
    return result.stdout


def init_repo(path: Path):
    run_cli(["init"], path)


def extract_save_id(history_output: str, needle: str) -> str:
    for line in history_output.splitlines():
        if needle in line:
            return line.strip().split()[0]
    raise AssertionError(f"History entry containing '{needle}' not found:\n{history_output}")


def get_current_route(workdir: Path) -> str:
    output = run_cli(["route", "--list"], workdir)
    for line in output.splitlines():
        line = line.strip()
        if not line or line.startswith("Routes"):
            continue
        if "(current)" in line:
            return line.split()[0]
    return "master"
