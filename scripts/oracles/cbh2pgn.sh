#!/usr/bin/env sh
# Oracle runner: asdfjkl/cbh2pgn (MIT, Python) converts a classic .cbh to PGN.
#
# Oracle-only, by design: the tool is never linked or vendored into the library.
# It runs as a separate process and only when CBH_ORACLE=1.
#
# Usage: cbh2pgn.sh <db-base-or-.cbh> <out.pgn>
# Env:   CBH_ORACLE_PYTHON (default: vendor/oracles/venv/bin/python3)
#        CBH_ORACLE_CBH2PGN (default: vendor/oracles/cbh2pgn/cbh2pgn.py)
set -eu

[ "${CBH_ORACLE:-0}" = "1" ] || { echo "cbh2pgn: CBH_ORACLE=1 is required" >&2; exit 2; }

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
PY=${CBH_ORACLE_PYTHON:-"$ROOT/vendor/oracles/venv/bin/python3"}
TOOL=${CBH_ORACLE_CBH2PGN:-"$ROOT/vendor/oracles/cbh2pgn/cbh2pgn.py"}

[ -x "$PY" ] || { echo "cbh2pgn: python not found at $PY (see vendor/oracles)" >&2; exit 3; }
[ -f "$TOOL" ] || { echo "cbh2pgn: converter not found at $TOOL (clone asdfjkl/cbh2pgn)" >&2; exit 3; }

INPUT=$1
OUT=$2
case "$INPUT" in
  *.cbh) CBH=$INPUT ;;
  *)     CBH="$INPUT.cbh" ;;
esac

exec "$PY" "$TOOL" -i "$CBH" -o "$OUT"
