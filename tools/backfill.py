#!/usr/bin/env python3
"""Score past commits against today's fixed replay corpora, for the progress chart.
Builds each commit in a scratch worktree and writes metrics/backfill.jsonl.

  python3 -I tools/backfill.py WORKDIR [COMMIT ...]   # default: every commit
"""
import json, re, subprocess, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import metrics  # corpora()

ROOT = metrics.ROOT


def git(*a, cwd=ROOT):
    return subprocess.run(["git", *a], cwd=cwd, capture_output=True, text=True, check=True).stdout


def main():
    work = Path(sys.argv[1])
    commits = sys.argv[2:] or git("log", "--reverse", "--format=%h").split()
    wt = work / "wt"
    if not wt.exists():
        git("worktree", "add", "--detach", str(wt))
    out = ROOT / "metrics/backfill.jsonl"
    done = {json.loads(l)["commit"] for l in out.read_text().splitlines()} if out.exists() else set()
    corp = metrics.corpora()
    for c in commits:
        if c in done:
            continue
        git("checkout", "-q", "--detach", c, cwd=wt)
        b = subprocess.run(["cargo", "build", "--release", "-q"], cwd=wt, capture_output=True, text=True,
                           env={**__import__("os").environ, "CARGO_TARGET_DIR": str(work / "target")})
        row = {"commit": c, "time": git("show", "-s", "--format=%cI", c).strip(),
               "subject": git("show", "-s", "--format=%s", c).strip(), "replay": {}}
        if b.returncode != 0:
            row["error"] = "build failed"
        else:
            binp = work / "target/release/awbw-replay"
            for name, dirs in corp.items():
                r = subprocess.run([str(binp), *dirs], capture_output=True, text=True)
                m = re.search(r"turns compared: (\d+)\s+clean: (\d+)", r.stdout)
                a = re.search(r"all checks: ([\d.]+)% of (\d+)", r.stdout)
                row["replay"][name] = ({"turns": int(m.group(1)), "clean": int(m.group(2)),
                                        "checks_pct": float(a.group(1)) if a else None,
                                        "checks": int(a.group(2)) if a else None}
                                       if m else {"error": f"exit {r.returncode}"})
        print(json.dumps(row), flush=True)
        with open(out, "a") as f:
            f.write(json.dumps(row) + "\n")


if __name__ == "__main__":
    main()
