from pathlib import Path

from .utils import get_current_route, init_repo, require_binary, run_cli


def test_route_switch_isolates_files(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    shared_file = tmp_path / "progress.log"
    shared_file.write_text("main route progress")
    run_cli(["save", "main baseline"], tmp_path)
    base_route = get_current_route(tmp_path)

    run_cli(["route", "switch", "-c", "alt-route"], tmp_path)
    (tmp_path / "alt-only.txt").write_text("alt content")
    shared_file.write_text("alt route progress")
    run_cli(["save", "alt checkpoint"], tmp_path)

    run_cli(["route", "switch", base_route], tmp_path)
    assert not (tmp_path / "alt-only.txt").exists()
    assert shared_file.read_text() == "main route progress"
