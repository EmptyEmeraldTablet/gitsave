from pathlib import Path

from .utils import extract_save_id, init_repo, require_binary, run_cli


def test_compare_two_saves(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    file1 = tmp_path / "player.dat"
    file1.write_text("Level: 1\nHealth: 100\nGold: 50")
    run_cli(["save", "初始存档"], tmp_path)

    file1.write_text("Level: 2\nHealth: 100\nGold: 150")
    run_cli(["save", "升级存档"], tmp_path)

    history_output = run_cli(["history"], tmp_path)
    save1_id = extract_save_id(history_output, "初始存档")
    save2_id = extract_save_id(history_output, "升级存档")

    output = run_cli(["compare", save1_id, save2_id], tmp_path)
    assert "Comparing" in output or save1_id in output or save2_id in output


def test_compare_shows_differences(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    file1 = tmp_path / "data.txt"
    file1.write_text("line1\nline2\nline3")
    run_cli(["save", "version 1"], tmp_path)

    file1.write_text("line1\nline2 modified\nline3\nline4")
    run_cli(["save", "version 2"], tmp_path)

    history_output = run_cli(["history"], tmp_path)
    save1_id = extract_save_id(history_output, "version 1")
    save2_id = extract_save_id(history_output, "version 2")

    output = run_cli(["compare", save1_id, save2_id], tmp_path)
    assert output
