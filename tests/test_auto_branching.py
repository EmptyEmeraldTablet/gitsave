"""Tests for automatic route creation when saving from detached history."""

from pathlib import Path

from .utils import extract_save_id, init_repo, require_binary, run_cli


def list_route_entries(workdir: Path):
    output = run_cli(["route", "--list"], workdir)
    entries = []
    for line in output.splitlines():
        line = line.strip()
        if not line or line.startswith("Routes"):
            continue
        name = line.split()[0]
        entries.append((name, "(current)" in line))
    return entries


def test_save_after_rollback_creates_fork_route(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    (tmp_path / "alpha.txt").write_text("alpha v1")
    run_cli(["save", "first checkpoint"], tmp_path)

    (tmp_path / "beta.txt").write_text("beta v1")
    run_cli(["save", "second checkpoint"], tmp_path)

    history = run_cli(["history"], tmp_path)
    first_save = extract_save_id(history, "first checkpoint")

    run_cli(["load", first_save], tmp_path)
    assert not (tmp_path / "beta.txt").exists(), "beta should be removed after rollback"

    (tmp_path / "gamma.txt").write_text("gamma branch content")
    run_cli(["save", "third checkpoint"], tmp_path)

    routes = list_route_entries(tmp_path)
    assert len(routes) >= 2, f"expected at least two routes, saw {routes}"

    fork_routes = [name for name, _ in routes if name.startswith("gitsave/")]
    assert fork_routes, f"missing auto-created route in {routes}"

    current_routes = [name for name, is_current in routes if is_current]
    assert current_routes and current_routes[0].startswith(
        "gitsave/"
    ), f"current route not switched to fork: {routes}"

    # Ensure only a single gitsave prefix is present to avoid recursive naming
    for route in fork_routes:
        assert route.count("gitsave/") == 1, f"malformed route name detected: {route}"

