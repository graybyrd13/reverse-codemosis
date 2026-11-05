#!/usr/bin/env sh
# The field test: `codemosis check` on open-source repositories pinned to
# exact commits, with a summary of what it found and how long it took.
# Only `check` runs: nothing in the repositories is executed or changed.
#
#   scripts/field-test.sh [WORKDIR]     (default: target/field-test)
#
# Each repository is fetched once (shallow, at its pinned commit). The JSON
# reports land next to the clones. docs/FIELD_TEST.md records the results.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
WORK=${1:-$ROOT/target/field-test}
BIN="$ROOT/target/release/codemosis"
(cd "$ROOT" && cargo build --release --quiet)
mkdir -p "$WORK"

# name  repository  commit
PINS=$(cat <<'PINS_END'
requests psf/requests 3c7baaa05ab4aa49cc819ce3a819d260de19c157
flask pallets/flask 49943b5a94413a530761350f04e896f76e8d620e
click pallets/click 2247b35ea1c47c727d7a06e51fa280e12a863ff6
httpx encode/httpx b5addb64f0161ff6bfe94c124ef76f6a1fba5254
rich Textualize/rich 9d8f9a372cc5916fd4781fec207ced7ddac2f08f
django django/django a7629749fa2886d0dcb8ddf2df1842fd85511093
express expressjs/express 43fca52d28a422c8be48fd489cbd4f2d6152bab6
axios axios/axios fb88a1b133c1a2d774713070b8ca25f7bcd382ac
chalk chalk/chalk 72110019e147683f60e13fa0ff5c14a0c27cd9fe
ms vercel/ms 4ff48cec099f0514c3e9bbca18706c9c21122bfb
cobra spf13/cobra adbc8813901bba65827259daa8e22ff94ec1f30e
gin gin-gonic/gin 0f09c3a9b4626d9fe9979cf4d0521d6ae32ab646
ripgrep BurntSushi/ripgrep 91f5d22ef48bde459d90a8c7dbeab416e97d0f2b
gson google/gson 65df6e5a99747f73d386f3a61d887a0257255e0f
sinatra sinatra/sinatra cb22afd7902b566b6eaba6c4ea89739494a65d12
guzzle guzzle/guzzle 7d9b1f430bb65ac664e120af227bf9a795ad6333
jq jqlang/jq 5c001b18d7237cce1fdba63608732163f978dd23
nvm nvm-sh/nvm 4e8758a801eea9479b32a904f31093e410ddaba4
Alamofire Alamofire/Alamofire 6c2892ea5adb5a03fa81c510933dd782c5ddd9cb
okio square/okio 1926f022425ccd85ee558c558f0a5b7b24826a57
PINS_END
)

printf '%-10s %6s %8s %9s %9s %10s %8s %7s\n' repo files proven questions explained suppressed unparsed seconds
echo "$PINS" | while read -r name repo sha; do
  dir="$WORK/$name"
  if [ ! -d "$dir/.git" ]; then
    git init -q "$dir"
    git -C "$dir" fetch -q --depth 1 "https://github.com/$repo.git" "$sha"
    git -C "$dir" checkout -q FETCH_HEAD
  fi
  start=$(date +%s.%N)
  "$BIN" check "$dir" --format json --fail-on never --timings > "$WORK/$name.json" 2> "$WORK/$name.err"
  end=$(date +%s.%N)
  python3 -I - "$WORK/$name.json" "$WORK/$name.err" "$name" "$start" "$end" <<'PY'
import json, re, sys
report, err, name, start, end = sys.argv[1:]
d = json.load(open(report))
files = re.search(r"(\d+) file\(s\) read", open(err).read())
count = lambda kind: sum(1 for f in d["findings"] if f["status"]["kind"] == kind)
print(f"{name:<10} {files.group(1) if files else '?':>6} {count('fixable'):>8} {count('needs_answer'):>9} "
      f"{count('explained'):>9} {count('suppressed'):>10} {len(d['parse_hazards']):>8} {float(end) - float(start):>7.2f}")
PY
done
