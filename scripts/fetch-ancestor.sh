#!/usr/bin/env sh
# Fetch the pinned upstream ancestor (cbformat) into vendor/upstream-snapshot/.
# Local-only: vendor/ is git-ignored; nothing here is redistributed.
# The commit is the one recorded in docs/port-inventory.md (task 0.1).
set -eu

COMMIT=ca9e8f8e4389edd6430f02a14b33ff53552bcadc
REPO=https://github.com/asavis/oschess-cb-bridge
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
DEST="$ROOT/vendor/upstream-snapshot"

if [ -e "$DEST/.git" ] && [ "$(git -C "$DEST" rev-parse HEAD 2>/dev/null)" = "$COMMIT" ]; then
  echo "upstream-snapshot already at $COMMIT"
  exit 0
fi

rm -rf "$DEST"
mkdir -p "$DEST"
git -C "$DEST" init -q
git -C "$DEST" remote add origin "$REPO"
git -C "$DEST" fetch --depth 1 origin "$COMMIT"
git -C "$DEST" checkout -q FETCH_HEAD
echo "upstream-snapshot at $(git -C "$DEST" rev-parse HEAD)"
