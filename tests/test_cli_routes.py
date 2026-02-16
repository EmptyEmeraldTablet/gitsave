from pathlib import Path

from .utils import extract_save_id, init_repo, require_binary, run_cli


def test_route_list_flag_after_complex_history(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    (tmp_path / "alpha.txt").write_text("alpha-v1")
    run_cli(["save", "first save"], tmp_path)

    (tmp_path / "beta.txt").write_text("beta-added")
    run_cli(["save", "second save"], tmp_path)

    history_output = run_cli(["history"], tmp_path)
    first_save_id = extract_save_id(history_output, "first save")

    run_cli(["load", first_save_id], tmp_path)
    assert not (tmp_path / "beta.txt").exists(), "beta.txt should be absent after rollback"

    (tmp_path / "gamma.txt").write_text("gamma fresh")
    run_cli(["save", "third save"], tmp_path)

    output = run_cli(["route", "--list"], tmp_path)
    assert "Routes:" in output
    assert any(branch in output for branch in ("main", "master"))
