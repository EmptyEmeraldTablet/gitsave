from pathlib import Path

from .utils import extract_save_id, init_repo, require_binary, run_cli


def test_save_and_load_restores_file(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    save_file = tmp_path / "player.dat"
    save_file.write_text("level 10")
    run_cli(["save", "first checkpoint"], tmp_path)

    save_file.write_text("level 20")
    run_cli(["save", "second checkpoint"], tmp_path)

    history_output = run_cli(["history"], tmp_path)
    first_id = extract_save_id(history_output, "first checkpoint")

    run_cli(["load", first_id], tmp_path)
    assert save_file.read_text() == "level 10"
