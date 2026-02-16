from pathlib import Path

from .utils import init_repo, require_binary, run_cli


def test_config_set_and_get(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    run_cli(["config", "--set", "save.max_history=100"], tmp_path)

    output = run_cli(["config"], tmp_path)
    assert "max_history = 100" in output or "max_history=100" in output


def test_autosave_enable_and_status(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    run_cli(
        ["autosave", "--enable", "--interval", "120", "--max-count", "20"], tmp_path
    )

    output = run_cli(["autosave", "--status"], tmp_path)
    assert "Enabled: yes" in output or "enabled" in output.lower()


def test_autosave_disable(tmp_path: Path):
    require_binary()
    init_repo(tmp_path)

    run_cli(["autosave", "--enable", "--interval", "60"], tmp_path)
    run_cli(["autosave", "--disable"], tmp_path)

    output = run_cli(["autosave", "--status"], tmp_path)
    assert "Enabled: no" in output or "disabled" in output.lower()
