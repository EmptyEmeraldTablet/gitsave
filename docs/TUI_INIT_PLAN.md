# TUI Init Flow Plan

## Scope (Phase 1)
- Add a TUI initialization flow when no Git repository exists.
- Allow the user to input a target path, preview directory contents, confirm, then initialize.
- If no Git signature is available, prompt for author name/email (optional) and write to gitsave.toml.

## Scope (Phase 2)
- Add a recent-path picker shown before opening the TUI.
- Allow selecting current directory, recent paths, or a new path input.
- Persist recent paths to an OS-level config file.

## Non-goals (Phase 1)
- Recent-path cache.
- Export or cleanup features.
- Recovery-mode configuration toggles.

## UX Flow
1. **Detect missing repo**
   - On `gitsave tui` launch, if `.git` is missing, enter the init screen.

2. **Path input**
   - User edits the target path.
   - `Enter` scans the path and shows a preview list.
   - Invalid path -> show error and stay in input.

3. **Preview & confirm**
   - Show directory entries (limited list + counts).
   - `Y` = proceed, `N`/`Esc` = go back to path input.

4. **Author prompt (conditional)**
   - If repo has no signature, prompt for `author.name` and `author.email`.
   - `Enter` confirms, `Esc` skips (leave empty).

5. **Init & commit**
   - Create repo, write root `gitsave.toml`, commit only that file.
   - If init succeeds, continue into normal TUI.

## Config Output
- Create `gitsave.toml` at repo root:
  - `[save]`, `[auto_save]`, `[author]` sections.

## Safety Guards
- Explicit confirmation step before init.
- Never delete or modify user files during init.
- Commit only `gitsave.toml` in the initial commit.

## Acceptance Criteria
- TUI launches into init screen when no repo exists.
- User can input a path and see a file preview list.
- Init writes and commits `gitsave.toml` to the chosen path.
- Optional author prompt appears only when no Git signature is detected.
- After init, normal TUI screen loads without restarting.
