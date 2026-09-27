#!/usr/bin/env sh
# Oracle runner: scidb's cbh2si4 (GPL-2.0) converts a classic .cbh to .si4.
#
# Oracle-only, by design: the tool is never linked or vendored into the library.
# It runs as a separate process and only when CBH_ORACLE=1. `cbh2si4` is not
# installed on this machine by default; the script fails with a clear message.
#
# Usage: scidb.sh <db-base-or-.cbh> <out.si4>
# Env:   CBH_ORACLE_CBH2SI4 (the cbh2si4 executable)
set -eu

[ "${CBH_ORACLE:-0}" = "1" ] || { echo "scidb: CBH_ORACLE=1 is required" >&2; exit 2; }

TOOL=${CBH_ORACLE_CBH2SI4:-cbh2si4}
command -v "$TOOL" >/dev/null 2>&1 || { echo "scidb: $TOOL not found (install scidb or set CBH_ORACLE_CBH2SI4)" >&2; exit 3; }

INPUT=$1
OUT=$2
case "$INPUT" in
  *.cbh) CBH=$INPUT ;;
  *)     CBH="$INPUT.cbh" ;;
esac

"$TOOL" "$CBH" "$OUT"
