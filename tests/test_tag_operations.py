from pathlib import Path

from .utils import extract_save_id, init_repo, require_binary, run_cli


def test_tag_create_and_list(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    (tmp_path / "player.dat").write_text("Level: 1\nHealth: 100")
    run_cli(["save", "initial save"], tmp_path)

    run_cli(["tag", "important_milestone", "做出最终选择前的存档"], tmp_path)

    output = run_cli(["tag", "--list"], tmp_path)
    assert "important_milestone" in output


def test_tag_delete(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    (tmp_path / "player.dat").write_text("Level: 1")
    run_cli(["save", "test save"], tmp_path)

    run_cli(["tag", "to_delete", "temporary tag"], tmp_path)

    output = run_cli(["tag", "--list"], tmp_path)
    assert "to_delete" in output

    run_cli(["tag", "--delete", "to_delete"], tmp_path)

    output = run_cli(["tag", "--list"], tmp_path)
    assert "to_delete" not in output


def test_load_by_tag(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    save_file = tmp_path / "player.dat"
    save_file.write_text("before checkpoint")
    run_cli(["save", "checkpoint save"], tmp_path)

    run_cli(["tag", "checkpoint", "game checkpoint"], tmp_path)

    save_file.write_text("after checkpoint - new content")
    run_cli(["save", "after checkpoint save"], tmp_path)

    run_cli(["load", "--tag", "checkpoint"], tmp_path)
    assert save_file.read_text() == "before checkpoint"


def test_tag_across_routes(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    (tmp_path / "player.dat").write_text("main route data")
    run_cli(["save", "main save"], tmp_path)
    run_cli(["tag", "shared_tag", "shared across routes"], tmp_path)

    run_cli(["route", "switch", "-c", "alt_route"], tmp_path)

    output = run_cli(["tag", "--list"], tmp_path)
    assert "shared_tag" in output
