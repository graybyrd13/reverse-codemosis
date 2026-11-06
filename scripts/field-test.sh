#!/usr/bin/env sh
# The field test: `codemosis check` on open-source repositories pinned to
# exact commits, with a summary of what it found and how long it took.
# Only `check` runs: nothing in the repositories is executed or changed.
#
#   scripts/field-test.sh [--summary-json FILE] [--compare BASELINE] [WORKDIR]
#
# WORKDIR defaults to target/field-test. Each repository is fetched once
# (shallow, at its pinned commit); the JSON reports land next to the clones.
#
# --summary-json FILE  write the machine-readable summary (the precision
#                      ledger): counts per repository and rule, and every
#                      proven correction by fingerprint.
# --compare BASELINE   compare with a committed summary and exit 1 on a
#                      proven correction nobody has reviewed, or a rise of
#                      more than 10% in questions or explanations per
#                      thousand files. scripts/field-test-baseline.json is
#                      the reviewed baseline; docs/FIELD_TEST.md the record.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
WORK=$ROOT/target/field-test
SUMMARY=""
COMPARE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --summary-json) SUMMARY=$2; shift 2 ;;
    --compare) COMPARE=$2; shift 2 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) WORK=$1; shift ;;
  esac
done
BIN="$ROOT/target/release/codemosis"
(cd "$ROOT" && cargo build --release --quiet)
mkdir -p "$WORK"

PINS=$(grep -v '^#' "$ROOT/scripts/field-test-pins.txt")

echo "$PINS" > "$WORK/pins.txt"
"$BIN" --version | awk '{print $2}' > "$WORK/engine-version.txt"
echo "$PINS" | while read -r name repo sha; do
  dir="$WORK/$name"
  if [ ! -d "$dir/.git" ]; then
    git init -q "$dir"
    git -C "$dir" fetch -q --depth 1 "https://github.com/$repo.git" "$sha"
    git -C "$dir" checkout -q FETCH_HEAD
  fi
  python3 -I - "$BIN" "$dir" "$WORK/$name" <<'PY'
import subprocess, sys, time
binary, project, out = sys.argv[1:]
start = time.monotonic()
with open(out + ".json", "w") as o, open(out + ".err", "w") as e:
    subprocess.run([binary, "check", project, "--format", "json", "--fail-on", "never", "--timings"], stdout=o, stderr=e)
open(out + ".seconds", "w").write(f"{time.monotonic() - start:.3f}\n")
PY
done

set -- "$WORK"
[ -n "$SUMMARY" ] && set -- "$@" --summary-json "$SUMMARY"
[ -n "$COMPARE" ] && set -- "$@" --compare "$COMPARE"
exec python3 -I "$ROOT/scripts/field_test_summary.py" "$@"
