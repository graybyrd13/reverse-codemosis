"""The field test's precision ledger.

Reads the reports `scripts/field-test.sh` left in WORKDIR, prints the
results table, and optionally writes a machine-readable summary and compares
it with a committed baseline.

    python3 -I scripts/field_test_summary.py WORKDIR
        [--summary-json FILE] [--compare BASELINE]

The comparison fails (exit status 1) when:

- a proven correction appears that is not in the baseline: every proven
  correction in the baseline was read and judged correct, and a new one
  must be too, so it fails until the baseline is updated in a reviewed
  change;
- questions or explanations per thousand files rise by more than 10%;
- more files fail to parse, or a rule errors on real code.

Corrections that disappear and noise that falls are reported, not failed:
they lower recall or raise precision, and the baseline should be updated to
lock the gain in.
"""

import json
import re
import sys
from pathlib import Path

SCHEMA = "codemosis/field-test/v1"
TOLERANCE = 0.10


def load_repo(work, name, commit):
    report = json.loads((work / f"{name}.json").read_text())
    err = (work / f"{name}.err").read_text(errors="replace")
    files = re.search(r"(\d+) file\(s\) read", err)
    seconds_file = work / f"{name}.seconds"
    seconds = float(seconds_file.read_text()) if seconds_file.exists() else None
    findings = report["findings"]
    count = lambda kind: sum(1 for f in findings if f["status"]["kind"] == kind)
    by_rule = {}
    for f in findings:
        kind = f["status"]["kind"]
        if kind in ("fixable", "needs_answer", "explained", "refused"):
            entry = by_rule.setdefault(f["rule"], {"proven": 0, "questions": 0, "explained": 0, "refused": 0})
            key = {"fixable": "proven", "needs_answer": "questions"}.get(kind, kind)
            entry[key] += 1
    proven = sorted(
        (
            {
                "rule": f["rule"],
                "file": f["file"],
                "line": f["span"]["start"]["line"],
                "fingerprint": f["fingerprint"],
            }
            for f in findings
            if f["status"]["kind"] == "fixable"
        ),
        key=lambda p: (p["file"], p["line"], p["rule"]),
    )
    return {
        "commit": commit,
        "files": int(files.group(1)) if files else None,
        "proven": count("fixable"),
        "questions": count("needs_answer"),
        "explained": count("explained"),
        "refused": count("refused"),
        "suppressed": count("suppressed"),
        "unparsed": len(report["parse_hazards"]),
        "rule_errors": len(report["rule_errors"]),
        "by_rule": dict(sorted(by_rule.items())),
        "proven_corrections": proven,
    }, seconds


def totals(repos):
    t = {k: sum(r[k] or 0 for r in repos.values()) for k in
         ("files", "proven", "questions", "explained", "refused", "suppressed", "unparsed", "rule_errors")}
    kfiles = max(t["files"], 1) / 1000
    t["questions_per_kfile"] = round(t["questions"] / kfiles, 2)
    t["explained_per_kfile"] = round(t["explained"] / kfiles, 2)
    return t


def compare(current, baseline):
    """Lines to print, and whether the comparison passes."""
    lines, ok = [], True
    common = sorted(set(current["repositories"]) & set(baseline["repositories"]))
    for name in sorted(set(baseline["repositories"]) - set(current["repositories"])):
        lines.append(f"  note: {name} is in the baseline but was not run")
    for name in sorted(set(current["repositories"]) - set(baseline["repositories"])):
        lines.append(f"  note: {name} is new; add it to the baseline once its results are reviewed")
    for name in common:
        if current["repositories"][name]["commit"] != baseline["repositories"][name]["commit"]:
            lines.append(f"  note: {name} is pinned to a different commit than the baseline")
    cur = {n: current["repositories"][n] for n in common}
    base = {n: baseline["repositories"][n] for n in common}
    ct, bt = totals(cur), totals(base)

    def row(label, b, c):
        lines.append(f"  {label:<30} {b:>8} → {c:<8}")

    new, gone = [], []
    for name in common:
        bp = {p["fingerprint"]: p for p in base[name]["proven_corrections"]}
        cp = {p["fingerprint"]: p for p in cur[name]["proven_corrections"]}
        new += [(name, cp[f]) for f in cp if f not in bp]
        gone += [(name, bp[f]) for f in bp if f not in cp]
    row("proven corrections", bt["proven"], ct["proven"])
    row("questions per 1000 files", bt["questions_per_kfile"], ct["questions_per_kfile"])
    row("explanations per 1000 files", bt["explained_per_kfile"], ct["explained_per_kfile"])
    row("corrections refused", bt["refused"], ct["refused"])
    row("files the grammar cannot parse", bt["unparsed"], ct["unparsed"])
    row("rule errors", bt["rule_errors"], ct["rule_errors"])

    if new:
        ok = False
        lines.append(f"FAIL: {len(new)} proven correction(s) not in the baseline; read each one, then update the baseline:")
        lines += [f"    {n} {p['file']}:{p['line']}  {p['rule']}" for n, p in new]
    if gone:
        lines.append(f"  {len(gone)} proven correction(s) in the baseline are no longer made (lower recall; update the baseline if intended):")
        lines += [f"    {n} {p['file']}:{p['line']}  {p['rule']}" for n, p in gone]
    for key, label in (("questions_per_kfile", "questions"), ("explained_per_kfile", "explanations")):
        if ct[key] > bt[key] * (1 + TOLERANCE) and ct[key] - bt[key] >= 0.01:
            ok = False
            rises = []
            for name in common:
                for rule, e in cur[name]["by_rule"].items():
                    b = base[name]["by_rule"].get(rule, {})
                    k = "questions" if label == "questions" else "explained"
                    if e[k] > b.get(k, 0):
                        rises.append(f"    {name} {rule}: {b.get(k, 0)} → {e[k]}")
            lines.append(f"FAIL: {label} per 1000 files rose more than {TOLERANCE:.0%}; state why, then update the baseline:")
            lines += rises
        elif ct[key] < bt[key]:
            lines.append(f"  {label} fell: update the baseline to keep the gain")
    if ct["unparsed"] > bt["unparsed"]:
        ok = False
        lines.append("FAIL: more files fail to parse than in the baseline")
    elif ct["unparsed"] < bt["unparsed"]:
        lines.append("  fewer files fail to parse: update the baseline to keep the gain")
    if ct["rule_errors"] > bt["rule_errors"]:
        ok = False
        lines.append("FAIL: a rule errors on real code (see the .json reports' rule_errors)")
    return lines, ok


def main(argv):
    args = list(argv)
    summary_path = compare_path = None
    if "--summary-json" in args:
        i = args.index("--summary-json")
        summary_path = args[i + 1]
        del args[i:i + 2]
    if "--compare" in args:
        i = args.index("--compare")
        compare_path = args[i + 1]
        del args[i:i + 2]
    if len(args) != 1:
        print(__doc__, file=sys.stderr)
        return 2
    work = Path(args[0])
    pins = [line.split() for line in (work / "pins.txt").read_text().splitlines() if line.strip()]
    version = (work / "engine-version.txt").read_text().strip() if (work / "engine-version.txt").exists() else None

    repos, seconds = {}, {}
    print(f"{'repo':<10} {'files':>6} {'proven':>8} {'questions':>9} {'explained':>9} "
          f"{'suppressed':>10} {'unparsed':>8} {'seconds':>7}")
    for name, _repo, commit in pins:
        r, s = load_repo(work, name, commit)
        repos[name], seconds[name] = r, s
        print(f"{name:<10} {r['files'] or '?':>6} {r['proven']:>8} {r['questions']:>9} {r['explained']:>9} "
              f"{r['suppressed']:>10} {r['unparsed']:>8} {s if s is None else f'{s:.2f}':>7}")
    t = totals(repos)
    total_s = sum(s for s in seconds.values() if s)
    print(f"{'total':<10} {t['files']:>6} {t['proven']:>8} {t['questions']:>9} {t['explained']:>9} "
          f"{t['suppressed']:>10} {t['unparsed']:>8} {total_s:>7.2f}")

    summary = {"schema": SCHEMA, "engine": version, "totals": t, "repositories": repos}
    if summary_path:
        Path(summary_path).write_text(json.dumps(summary, indent=2) + "\n")
        print(f"\nSummary written to {summary_path}")
    if compare_path:
        baseline = json.loads(Path(compare_path).read_text())
        if baseline.get("schema") != SCHEMA:
            print(f"{compare_path} is not a {SCHEMA} summary", file=sys.stderr)
            return 2
        lines, ok = compare(summary, baseline)
        print(f"\nPrecision ledger against {compare_path}")
        print("\n".join(lines))
        print("ok: no unreviewed correction and no unexplained rise in noise" if ok else "the ledger does not balance")
        return 0 if ok else 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
