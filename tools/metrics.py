#!/usr/bin/env python3
"""Measure the engine and append one line to metrics/history.jsonl. Runs locally: the
replay corpora live in ~/repos/awbw-engine-data (and awbw_chart's team games), not in git.

  python3 -I tools/metrics.py            # print and append
  python3 -I tools/metrics.py --dry-run  # print only
  python3 -I tools/metrics.py --check    # print, then exit 1 if any corpus has fewer
                                         # clean turns than metrics/baseline.json (CI)

AWBW_DATA and AWBW_TEAM override the corpus locations (CI unpacks the private
awbw-engine-data repo).

Metrics (one JSON object per run):
  replay.<corpus>: turns compared, clean turns, checks passed/total. Engine fidelity:
    how closely our rules reproduce recorded AWBW games.
  legal: recorded actions the generator offers / recorded actions checked (completeness).
  selfplay: random-vs-random games from the test states: actions/s, step errors, broken
    invariants (simulator health).
  powers: recorded CO power activations whose instant effects we reproduce ("match"), get
    wrong ("differ"), or don't model yet ("missing", by CO and power). Tracked apart from
    rules fidelity because it grows as powers are implemented.
  spend_today: Claude spend so far today (tools/spend.py).
"""
import json, os, re, subprocess, sys, time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "target/release/awbw-replay"
DATA = Path(os.environ.get("AWBW_DATA", Path.home() / "repos/awbw-engine-data"))
TEAM = Path(os.environ.get("AWBW_TEAM", Path.home() / "repos/awbw_chart/data"))


def done_dirs(ids):
    return [str(DATA / f"replays/game-{i}") for i in ids if (DATA / f"replays/game-{i}/.done").exists()]


def corpora():
    frozen = json.loads((ROOT / "metrics/corpora.json").read_text())
    b1, b2 = frozen["league"], frozen["league_batch2"]
    seen = set(b1) | set(b2)
    b3 = [x["id"] for x in json.loads((DATA / "league_std_pages1-5.json").read_text()) if x["id"] not in seen]
    out = {
        "league": done_dirs(b1),
        "league_batch2": done_dirs(b2),
        # Every other Global League standard game on pages 1-5 (fetched 2026-11-04 on).
        "league_more": done_dirs(b3),
        "team": sorted(str(p) for p in TEAM.glob("game-*") if p.is_dir()),
    }
    return {k: v for k, v in out.items() if v}


def run_checker(dirs):
    if not dirs:
        sys.exit("metrics: empty corpus (data not unpacked?)")
    run = subprocess.run([str(BIN), *dirs], capture_output=True, text=True)
    out = run.stdout
    m = re.search(r"turns compared: (\d+)\s+clean: (\d+)", out)
    if not m:
        sys.exit(f"metrics: checker gave no summary (exit {run.returncode}):\n{out[-2000:]}\n{run.stderr[-3000:]}")
    checks = {}
    for line in out.splitlines():
        c = re.match(r"(\S+)\s+(\d+) ok\s+(\d+) bad", line)
        if c:
            checks[c.group(1)] = (int(c.group(2)), int(c.group(3)))
    ok = sum(a for a, _ in checks.values())
    bad = sum(b for _, b in checks.values())
    missing = {}
    for mm in re.finditer(r"skipped: power effects not modelled: (.+?) ([YS]) x(\d+)", out):
        k = f"{mm.group(1)} {'COP' if mm.group(2) == 'Y' else 'SCOP'}"
        missing[k] = missing.get(k, 0) + int(mm.group(3))
    pe = checks.get("power.effects", (0, 0))
    return {"turns": int(m.group(1)), "clean": int(m.group(2)), "checks_ok": ok, "checks_total": ok + bad,
            "legal": {k: v for k, v in checks.items() if k.startswith("legal.")},
            "powers": {"match": pe[0], "differ": pe[1], "missing": missing}}


def run_selfplay():
    res = {"games": 0, "actions": 0, "step_errors": 0, "violations": 0, "actions_per_s": []}
    for f in sorted((ROOT / "docs/eval-harness/test-states").glob("*.json")):
        out = subprocess.run([str(BIN), "--selfplay", str(f), "2"], capture_output=True, text=True).stdout
        for m in re.finditer(r"(\d+) actions, .*?(\d+) step errors, (\d+) invariant violations, (\d+) actions/s", out):
            res["games"] += 1
            res["actions"] += int(m.group(1))
            res["step_errors"] += int(m.group(2))
            res["violations"] += int(m.group(3))
            res["actions_per_s"].append(int(m.group(4)))
    aps = res.pop("actions_per_s")
    res["actions_per_s"] = round(sum(aps) / len(aps)) if aps else 0
    return res


def main():
    subprocess.run(["cargo", "build", "--release", "-q"], cwd=ROOT, check=True)
    sha = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=ROOT, capture_output=True, text=True).stdout.strip()
    row = {"time": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "commit": sha, "replay": {}, "legal": {}}
    for name, dirs in corpora().items():
        r = run_checker(dirs)
        for k, (ok, bad) in r.pop("legal").items():
            a, b = row["legal"].get(k, (0, 0))
            row["legal"][k] = (a + ok, b + ok + bad)
        r["games"] = len(dirs)
        pw = r.pop("powers")
        P = row.setdefault("powers", {"match": 0, "differ": 0, "missing": {}})
        P["match"] += pw["match"]; P["differ"] += pw["differ"]
        for k, v in pw["missing"].items():
            P["missing"][k] = P["missing"].get(k, 0) + v
        row["replay"][name] = r
    row["legal"] = {k: {"ok": a, "total": b} for k, (a, b) in row["legal"].items()}
    row["selfplay"] = run_selfplay()
    spend = "" if os.environ.get("CI") else subprocess.run(
        [sys.executable, "-I", str(ROOT / "tools/spend.py")], capture_output=True, text=True).stdout
    m = re.search(r"spent today \$([\d.]+)", spend)
    row["spend_today"] = float(m.group(1)) if m else None
    print(json.dumps(row, indent=1))
    if "--check" in sys.argv:
        base = json.loads((ROOT / "metrics/baseline.json").read_text())
        bad = [f"{k}: {row['replay'].get(k, {}).get('clean', 0)} clean turns, baseline {v}"
               for k, v in base["clean"].items() if row["replay"].get(k, {}).get("clean", 0) < v]
        for b in bad:
            print("REGRESSION", b)
        sys.exit(1 if bad else 0)
    if "--dry-run" not in sys.argv:
        (ROOT / "metrics").mkdir(exist_ok=True)
        with open(ROOT / "metrics/history.jsonl", "a") as f:
            f.write(json.dumps(row) + "\n")


if __name__ == "__main__":
    main()
