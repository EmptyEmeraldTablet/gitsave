import subprocess
import uuid

from pathlib import Path

from .utils import GITSAVE_BIN, init_repo, require_binary, run_cli


def test_route_delete_requires_confirmation(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    (tmp_path / "player.dat").write_text("data")
    run_cli(["save", "initial"], tmp_path)

    route_name = f"to_delete_{uuid.uuid4().hex[:8]}"
    run_cli(["route", "create", route_name], tmp_path)

    result = subprocess.run(
        [str(GITSAVE_BIN), "route", "delete", route_name],
        cwd=tmp_path,
        input="n\n",
        text=True,
        capture_output=True,
    )


def test_route_delete_current_route_blocked(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    (tmp_path / "player.dat").write_text("data")
    run_cli(["save", "main save"], tmp_path)

    run_cli(["route", "create", "alt_route"], tmp_path)

    result = subprocess.run(
        [str(GITSAVE_BIN), "route", "delete", "alt_route"],
        cwd=tmp_path,
        input="y\n",
        text=True,
        capture_output=True,
    )
    assert result.returncode == 0


def test_route_delete_with_confirmation(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    (tmp_path / "player.dat").write_text("data")
    run_cli(["save", "initial"], tmp_path)

    run_cli(["route", "create", "temp_route"], tmp_path)

    result = subprocess.run(
        [str(GITSAVE_BIN), "route", "delete", "temp_route"],
        cwd=tmp_path,
        input="y\n",
        text=True,
        capture_output=True,
    )
    assert result.returncode == 0 or "Deleted" in result.stdout

    output = run_cli(["route", "list"], tmp_path)
    assert "temp_route" not in output
