#!/usr/bin/env sh
# The proof, end to end: one broken repository, one command, a green test
# suite, and a diff to read line by line.
#
#   scripts/demo.sh            # uses ./target/release/codemosis, building it if needed
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="$ROOT/target/release/codemosis"
if [ ! -x "$BIN" ]; then
  echo "Building codemosis..."
  (cd "$ROOT" && cargo build --release --quiet)
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
cp -R "$ROOT/examples/broken-inventory" "$WORK/broken-inventory"
cd "$WORK/broken-inventory"
git init -q
git add -A
git -c user.email=demo@example.com -c user.name=demo commit -qm "broken"

export PYTHONDONTWRITEBYTECODE=1

echo "== The test suite before =="
python3 -W ignore -m unittest discover -s tests -t . 2>&1 | tail -n 3 || true
echo

echo "== One command =="
echo "\$ codemosis fix"
"$BIN" fix
echo

echo "== The test suite after =="
python3 -W ignore -m unittest discover -s tests -t . 2>&1 | tail -n 3
echo

echo "== The diff to review =="
git --no-pager diff --stat
git --no-pager diff
