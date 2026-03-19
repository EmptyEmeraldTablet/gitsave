#!/usr/bin/env bash

set -euo pipefail

GITSAVE_BIN="${GITSAVE_BIN:-/home/yolo_dev/nop/gamegit/gitsave/target/release/gitsave}"
if [ ! -x "$GITSAVE_BIN" ]; then
    echo "gitsave binary not found at $GITSAVE_BIN"
    echo "Build with: cargo build --release"
    exit 1
fi

BASE_DIR="${TMPDIR:-/tmp}/gitsave_discard_case_$(date +%s)"
SAVE_DIR="$BASE_DIR/saves"

cleanup() {
    if [ "${KEEP_DIR:-0}" -eq 0 ]; then
        rm -rf "$BASE_DIR"
    else
        echo "Keeping test dir: $BASE_DIR"
    fi
}
trap cleanup EXIT

mkdir -p "$SAVE_DIR"
cd "$SAVE_DIR"

echo "== init =="
"$GITSAVE_BIN" init

echo "== commit 1 =="
echo "slot A" > save.dat
"$GITSAVE_BIN" save "slot A"

echo "== commit 2 =="
echo "slot B" > save.dat
"$GITSAVE_BIN" save "slot B"

LATEST_ID=$(git rev-parse --short HEAD)
PREV_ID=$(git rev-parse --short HEAD~1)

echo "Latest: $LATEST_ID"
echo "Prev  : $PREV_ID"
echo "History:"
"$GITSAVE_BIN" history

echo
echo "== simulate game exit: delete save + add history file =="
rm -f save.dat
mkdir -p history
echo "run log" > history/1773902383.run

"$GITSAVE_BIN" status
git status --short

echo
echo "== simulate tui d (reset only) =="
git reset --hard HEAD
git status --short

echo
echo "== re-add history file after reset =="
echo "run log" > history/1773902383.run
git status --short

echo
echo "== discard via gitsave load --force $LATEST_ID =="
printf "y\n" | "$GITSAVE_BIN" load --force "$LATEST_ID"

"$GITSAVE_BIN" status
git status --short
ls -la save.dat history/1773902383.run 2>/dev/null || true

echo
echo "== load previous commit to restore =="
printf "y\n" | "$GITSAVE_BIN" load --force "$PREV_ID"

"$GITSAVE_BIN" status
git status --short
ls -la save.dat history/1773902383.run 2>/dev/null || true

echo
echo "== commit after deletion + new history file =="
rm -f save.dat
mkdir -p history
echo "run log 2" > history/1773902383.run
"$GITSAVE_BIN" save "after exit"

echo "Last commit files:"
git show --name-status --oneline -1
"$GITSAVE_BIN" status
git status --short
