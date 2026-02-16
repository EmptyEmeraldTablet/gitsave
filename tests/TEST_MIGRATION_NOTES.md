# Pytest Migration Roadmap

## Completed
- `test_cli_routes.py`: simulates the multi-step route/list scenario (save, add, rollback, new save) and verifies `gitsave route --list` output.
- `test_save_load.py`: basic save/load validation to ensure files roll back correctly.
- `test_route_switch.py`: confirms branch isolation when switching between main and new routes.
- `test_auto_branching.py`: reproduces the rollback → diverging save flow, asserts auto-created `gitsave/<base>/timestamp-NNN` route naming, and protects against recursive prefixes.
- Shared helpers in `tests/utils.py` for invoking `target/release/gitsave` from pytest.

## Next candidates
1. **Tag operations**: port `test_tag` (creation/list/delete) and `test_tag_load` from bash.
2. **Autosave configuration**: replicate `test_config` and `test_autosave` assertions against `gitsave autosave --status`.
3. **History diff & compare**: wrap `compare` CLI and inspect output for additions/deletions (using the new line stats).
4. **Route deletion safeguards**: ensure CLI blocks deleting the current route and prompts for confirmation.
5. **Export/Import workflow**: once the feature stabilises, use pytest tmp dirs to validate round-trip.

## Tips
- Prefer pytest fixtures (`tmp_path`) for isolated repos; helper functions already support this.
- Guard tests with `require_binary()` to skip when `target/release/gitsave` is missing.
- When porting bash tests, keep a 1:1 mapping between scenario description and pytest file to simplify tracking.
