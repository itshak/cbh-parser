#!/usr/bin/env sh
# Oracle runner: uncbv (GPL-3.0) lists or extracts a .cbv/.cbz archive.
#
# Oracle-only, by design: the tool is never linked or vendored into the library.
# It runs as a separate process and only when CBH_ORACLE=1.
# The clean-room protocol (docs/provenance.md) allows its *outputs* as facts;
# its source is never read.
#
# Usage: uncbv.sh list <archive.cbv>
#        uncbv.sh extract <archive.cbv> <out-dir>
# Env:   CBH_ORACLE_UNCBV (default: vendor/oracles/uncbv/target/release/uncbv)
set -eu

[ "${CBH_ORACLE:-0}" = "1" ] || { echo "uncbv: CBH_ORACLE=1 is required" >&2; exit 2; }

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
TOOL=${CBH_ORACLE_UNCBV:-"$ROOT/vendor/oracles/uncbv/target/release/uncbv"}
[ -x "$TOOL" ] || { echo "uncbv: binary not found at $TOOL (build vendor/oracles/uncbv)" >&2; exit 3; }

MODE=$1
ARCHIVE=$2
case "$MODE" in
  list)    exec "$TOOL" list "$ARCHIVE" ;;
  extract) exec "$TOOL" extract "$ARCHIVE" --output="${3:?output directory required}" --no-confirm ;;
  *)       echo "uncbv: mode must be 'list' or 'extract'" >&2; exit 2 ;;
esac
