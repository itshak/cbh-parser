#!/usr/bin/env bash
# Compare memory modes on a slice of a real database.
#
#   scripts/mem_modes.sh "<db base>" [first] [last] [threads]
#
# Modes (pgn export to /dev/null, single-threaded by default):
#   A  default      mmap on,  .cbj auto (skipped below 4 GiB after this change)
#   B  force-wide   mmap on,  .cbj forced (CBVAULT_WIDE=on: the old behaviour)
#   C  no-wide      mmap on,  .cbj off
#   D  no-mmap      pread,    .cbj auto
#   E  no-mmap+no-wide (smallest footprint)
#
# Reports wall seconds and peak RSS (macOS `time -l` or GNU `time -v`).
set -u
DB="${1:-Mega Database 2025/Mega Database 2025}"
FIRST="${2:-1}"
LAST="${3:-50000}"
THREADS="${4:-1}"
BIN="${CBVAULT_BIN:-target/release/cbvault}"

if [ ! -x "$BIN" ]; then
  echo "build it first: cargo build --release -p cbvault-cli" >&2
  exit 2
fi

have_l=false; /usr/bin/time -l true >/dev/null 2>&1 && have_l=true

run_mode() {
  local name="$1"; shift
  local out secs rss
  if [ "$have_l" = true ]; then
    out=$(/usr/bin/time -l env "$@" "$BIN" pgn "$DB" /dev/null \
      --from "$FIRST" --to "$LAST" --threads "$THREADS" 2>&1)
    secs=$(printf '%s' "$out" | grep -E 'exported .* in .* s' | sed -E 's/.* in ([0-9.]+) s.*/\1/')
    rss=$(printf '%s' "$out" | grep -E 'maximum resident set size' | awk '{print $1}')
    rss_mb=$(awk "BEGIN {printf \"%.0f\", $rss/1024/1024}")
    echo "$name: ${secs:-?} s, peak RSS ${rss_mb:-?} MB ($rss bytes)"
  else
    out=$(/usr/bin/time -v env "$@" "$BIN" pgn "$DB" /dev/null \
      --from "$FIRST" --to "$LAST" --threads "$THREADS" 2>&1)
    secs=$(printf '%s' "$out" | grep -E 'Elapsed.*wall clock' | sed -E 's/.* ([0-9:.]+) .*/\1/')
    rss=$(printf '%s' "$out" | grep -i 'maximum resident' | grep -oE '[0-9]+')
    echo "$name: $secs wall, peak RSS $rss kB"
  fi
}

echo "db=$DB games $FIRST..$LAST threads=$THREADS bin=$BIN"
run_mode "A default        " 
run_mode "B force-wide     " CBVAULT_WIDE=on
run_mode "C no-wide        " CBVAULT_NO_WIDE=1
run_mode "D no-mmap        " CBVAULT_NO_MMAP=1
run_mode "E neither        " CBVAULT_NO_MMAP=1 CBVAULT_NO_WIDE=1
