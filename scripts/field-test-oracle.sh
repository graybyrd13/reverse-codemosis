#!/usr/bin/env sh
# The field test with real oracles: `codemosis fix` on one pinned
# repository, with the repository's own build and tests deciding every step.
# Then, without trusting the engine:
#
#   1. `codemosis proof verify` replays the certificate;
#   2. the test command runs again, and must pass if it passed before;
#   3. files the formatter accepted before must still be accepted.
#
#   scripts/field-test-oracle.sh [--from GIT_DIR] NAME [WORKDIR]
#
# --from takes the repository from a local git directory at its HEAD
# instead of the pinned commit, to try the harness on your own project.
#
# THIS RUNS THE REPOSITORY'S CODE (its build and its tests). Run it only in a
# disposable environment: CI's field-test workflow does. Dependencies are
# fetched first, then the package managers are put offline for the run.
# Supported ecosystems: Go modules and Cargo.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
FROM=""
if [ "${1:-}" = "--from" ]; then
  FROM=$(cd "$2" && pwd)
  shift 2
fi
NAME=${1:?usage: field-test-oracle.sh [--from GIT_DIR] NAME [WORKDIR]}
WORK=${2:-$ROOT/target/field-test-oracle}
BIN="$ROOT/target/release/codemosis"
(cd "$ROOT" && cargo build --release --quiet)

DIR="$WORK/$NAME"
rm -rf "$DIR"
mkdir -p "$DIR"
git -C "$DIR" init -q
if [ -n "$FROM" ]; then
  REPO=$FROM
  SHA=$(git -C "$FROM" rev-parse HEAD)
  git -C "$DIR" fetch -q "$FROM" HEAD
else
  PIN=$(grep -v '^#' "$ROOT/scripts/field-test-pins.txt" | awk -v n="$NAME" '$1 == n')
  [ -n "$PIN" ] || { echo "no pinned repository named $NAME" >&2; exit 2; }
  set -- $PIN
  REPO=$2
  SHA=$3
  git -C "$DIR" fetch -q --depth 1 "https://github.com/$REPO.git" "$SHA"
fi
git -C "$DIR" checkout -q FETCH_HEAD
cd "$DIR"

if [ -f go.mod ]; then
  BUILD="go build ./..."
  TEST="go test -v ./..."
  go mod download
  export GOFLAGS=-mod=readonly GOPROXY=off GOTOOLCHAIN=local
  formatted() { gofmt -l . | sort; }
elif [ -f Cargo.toml ]; then
  BUILD="cargo build --all-targets"
  TEST="cargo test"
  cargo fetch --quiet
  export CARGO_NET_OFFLINE=true
  formatted() { cargo fmt --all -- --check -l 2>/dev/null | sort || true; }
else
  echo "$NAME: no supported ecosystem (Go modules or Cargo)" >&2
  exit 2
fi

# The same commands for the engine's oracle and for the independent check,
# committed so the working tree is clean for `fix`.
cat > codemosis.toml <<EOF
[oracle]
build = "$BUILD"
test = "$TEST"
timeout = 1800
EOF
git add codemosis.toml
git -c user.name=field-test -c user.email=field-test@localhost commit -q -m "field test oracle"

echo "== $NAME ($REPO @ ${SHA%"${SHA#????????}"}): baseline"
sh -c "$TEST" > "$WORK/$NAME.baseline.log" 2>&1 && BASE_OK=1 || BASE_OK=0
echo "tests before: $([ $BASE_OK = 1 ] && echo pass || echo fail)"
UNFORMATTED_BEFORE=$(formatted)

echo "== $NAME: codemosis fix"
FIX_OK=1
"$BIN" fix --report "$WORK/$NAME.fix.md" --color never > "$WORK/$NAME.fix.txt" 2>&1 || FIX_OK=0
cat "$WORK/$NAME.fix.txt"
[ $FIX_OK = 1 ] || { echo "FAIL: codemosis fix did not complete" >&2; exit 1; }

STATUS=0
CERT=$(ls .codemosis/proofs/*.json 2>/dev/null | head -1 || true)
if [ -n "$CERT" ]; then
  echo "== $NAME: proof verify"
  "$BIN" proof verify "$CERT" --color never || STATUS=1
else
  echo "no correction was accepted; nothing to verify"
fi

echo "== $NAME: independent re-run of the tests"
sh -c "$TEST" > "$WORK/$NAME.after.log" 2>&1 && AFTER_OK=1 || AFTER_OK=0
echo "tests after: $([ $AFTER_OK = 1 ] && echo pass || echo fail)"
if [ $BASE_OK = 1 ] && [ $AFTER_OK = 0 ]; then
  echo "FAIL: the tests passed before the corrections and fail after them" >&2
  tail -40 "$WORK/$NAME.after.log" >&2
  STATUS=1
fi

echo "== $NAME: formatting"
UNFORMATTED_AFTER=$(formatted)
NEWLY=$(printf '%s\n' "$UNFORMATTED_AFTER" | grep -vxF -e "$UNFORMATTED_BEFORE" -e '' || true)
if [ -n "$NEWLY" ]; then
  echo "FAIL: files the formatter accepted before are no longer formatted:" >&2
  echo "$NEWLY" >&2
  STATUS=1
else
  echo "every file the formatter accepted before is still formatted"
fi

git diff --stat HEAD
git diff HEAD > "$WORK/$NAME.diff"
exit $STATUS
