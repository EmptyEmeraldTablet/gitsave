# GUI/TUI Flow Parity Plan

Goal: Make the GUI mirror the TUI flow and key-driven behavior, with mouse-first equivalents.
Scope: Flow parity only (layout may differ). All TUI key flows must have GUI buttons or clicks that follow the same state transitions, confirmations, and restrictions.

## Principles
- Match TUI state transitions (confirm, resolve dirty, resolve unstable, input prompts).
- Keep behavior identical for clean vs dirty working tree.
- Provide explicit force actions where TUI has force keys.
- Prevent actions that TUI blocks (recovery mode restrictions, rollback rules).

## TUI Flow Summary (Source of Truth)
- Save: SavePrompt -> optional default message -> save or resolve unstable.
- Force Save: SavePrompt (force) -> save_force.
- Switch route: confirm; dirty goes to ResolveDirty; force switch discards first.
- Rollback: only on current route; prompt for new route name; force rollback discards first.
- Create route / Create+Switch: input name -> confirm; dirty goes ResolveDirty.
- Rename route: input -> validate -> rename.
- Amend message: blocked if dirty.
- Recovery view: only recovery actions allowed; others blocked with guidance.
- Picker: Select/Input/Manage/ConfirmCleanup/ExportInput; manage shows info before open/init.

## GUI Parity Gaps (Flow)
- Save currently executes without prompt; no default message parity.
- Switch route lacks confirm on clean; no force switch action.
- Rollback allowed on non-current route; no force rollback action.
- Create/Create+Switch executes directly on clean (no confirm).
- Rename route lacks full validation and duplicate checks.
- Amend message not blocked on dirty.
- Recovery view does not block other actions.
- Picker manage lacks "init if missing" flow.

## Implementation Steps
1) Add unified confirmation flow for GUI actions.
   - Introduce confirm handling for pending actions (Create, Switch, Rollback, Recovery).
   - Ensure ResolveDirty -> Save/Discard/Cancel matches TUI.
2) Align save flow to TUI prompts.
   - Save/Force Save opens prompt modal and applies default messages if empty.
   - Block normal save when clean (notify like TUI).
3) Add force actions and restrictions.
   - Force switch and force rollback buttons, with TUI-equivalent behavior.
   - Enforce "rollback only on current route" rule.
4) Validate rename/create routes consistently.
   - Validate characters; block duplicates; no-op on same name.
5) Recovery view restrictions.
   - Disable unrelated actions while in recovery mode.
6) Picker manage parity.
   - Add "init if missing" action to manage panel.

## Acceptance Criteria
- Every TUI key flow has a GUI click-based equivalent.
- Clean vs dirty behavior matches TUI for all actions.
- Force actions are available and behave like TUI.
- Recovery view blocks unrelated actions.
- Route history and rollback rules match TUI.

## Out of Scope
- Pixel-perfect layout parity.
- Rewriting GUI to share the TUI state machine (future refactor).
