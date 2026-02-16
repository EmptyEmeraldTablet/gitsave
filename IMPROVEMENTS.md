# Proposed Improvements

This note captures follow-up items identified during the February 16, 2026 review. Each entry calls out the relevant module to help prioritize fixes.

1. **`SaveManager::load_by_tag` guard duplication**  
   `gitsave/src/manager/mod.rs` currently repeats the same `has_uncommitted_changes && !force` check three times (lines ~67–77). Collapse it into a single guard so future readers do not assume there are missing branches in the logic.

2. **History entries lose real route context**  
   `Git2Core::get_history` (`gitsave/src/git/mod.rs`) attributes every entry to the active branch at query time. When users play multiple “routes,” the history listing becomes misleading. Track the branch (or ref) that owns each commit while building entries, or store that metadata on save so UI can render accurate route information.

3. **Diff stats are coarse**  
   `Git2Core::compare_saves` counts “additions/deletions” as the number of files touched instead of line or hunk deltas. Consider using `diff.stats()` (with `StatsFormat::FULL`) or iterating patches to report actual added/deleted lines so players understand how large a change is.

4. **Config file location mismatches README/tests**  
   `handle_init` writes `gitsave.toml` under `.git/`, but README and `test_gitsave.sh` expect it beside the save files. Decide on a single location, update code/tests/docs together, and consider keeping `.gitignore` in sync.

5. **Auto-save lacks trigger mechanics**  
   Although the CLI exposes `gitsave autosave ...` and `SaveManager::should_auto_save`, nothing drives periodic saves. Define a cron/systemd helper, a background thread, or an API for games to poll `should_auto_save` and auto-trigger `save`, and mention the workflow in docs.

6. **Temporary auto-save tags accumulate**  
   `Git2Core::checkout` creates `_autosave_<timestamp>` tags on every load without cleanup. Move them under a dedicated namespace (e.g., `refs/tags/_gitsave/autosave/*`) or prune older tags to avoid polluting the tag list and preventing name reuse.

7. **Testing follow-up**  
   Run `./test_gitsave.sh` after implementing the above to verify the 60+ scenarios still pass, and add targeted unit tests where new behaviors (route metadata, diff stats, autosave triggers) are introduced.

Feel free to promote any of these into tracked GitHub issues if more visibility or discussion is needed.
