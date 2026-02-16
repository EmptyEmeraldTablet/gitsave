from pathlib import Path

from .utils import extract_save_id, init_repo, require_binary, run_cli


def test_rollback_then_reload_later_save(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    save1 = tmp_path / "save1.txt"
    save1.write_text("save1 content")
    run_cli(["save", "第一个存档"], tmp_path)

    save2 = tmp_path / "save2.txt"
    save2.write_text("save2 content")
    run_cli(["save", "第二个存档"], tmp_path)

    history = run_cli(["load", "--list"], tmp_path)
    assert "第一个存档" in history
    assert "第二个存档" in history

    first_save_id = extract_save_id(run_cli(["history"], tmp_path), "第一个存档")

    run_cli(["load", "--force", first_save_id], tmp_path)
    assert not save2.exists(), "save2.txt should be removed after rollback"
    assert save1.exists(), "save1.txt should still exist"

    history_after = run_cli(["load", "--list"], tmp_path)
    assert "第一个存档" in history_after
    assert "第二个存档" in history_after, "should still see all saves after rollback"

    second_save_id = extract_save_id(run_cli(["history"], tmp_path), "第二个存档")
    run_cli(["load", "--force", second_save_id], tmp_path)
    assert save2.exists(), "save2.txt should be restored"


def test_load_removes_new_files_on_rollback(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    base_file = tmp_path / "base_file.dat"
    base_file.write_text("Base content")
    run_cli(["save", "基础存档"], tmp_path)

    new_file = tmp_path / "new_file.dat"
    new_file.write_text("New file content")
    run_cli(["save", "添加新文件"], tmp_path)

    assert new_file.exists(), "new file should exist"

    base_save_id = extract_save_id(run_cli(["history"], tmp_path), "基础存档")
    run_cli(["load", "--force", base_save_id], tmp_path)

    assert not new_file.exists(), "new file should be removed after rollback"
    assert base_file.exists(), "base file should still exist"


def test_multi_level_rollback(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    level_file = tmp_path / "level.dat"
    for i in range(1, 4):
        level_file.write_text(f"Level {i} content")
        run_cli(["save", f"层级存档 {i}"], tmp_path)

    first_save_id = extract_save_id(run_cli(["history"], tmp_path), "层级存档 3")

    run_cli(["load", "--force", first_save_id], tmp_path)
    assert level_file.read_text() == "Level 3 content"
