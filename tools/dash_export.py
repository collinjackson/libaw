#!/usr/bin/env python3
"""Write the parity dashboard's data files to metrics/dash/ (CSV):

  history.csv  commit, time, subject, corpus, turns, clean   (from metrics/backfill.jsonl
               and metrics/history.jsonl: each commit scored on today's fixed corpora)
  checks.csv   check, area, ok, total                         (current build, all corpora)
  games.csv    game, corpus, cos, days, turns, clean          (current build, one run per game)

  python3 -I tools/dash_export.py
"""
import csv, json, re, subprocess
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
import metrics

ROOT = metrics.ROOT
OUT = ROOT / "metrics/dash"
AREA = {"fire": "Combat", "move": "Movement", "capt": "Capture", "state": "Board state",
        "turn": "Start of turn", "legal": "Legal moves", "join": "Join", "load": "Load/unload",
        "unload": "Load/unload", "repair": "Repair", "supply": "Supply", "power": "CO powers",
        "seam": "Pipe seams", "silo": "Missile silos", "build": "Builds", "delete": "Delete"}


def cos_of(d):
    f = sorted(Path(d).glob("turn-*.json"))[0]
    gs = json.loads(f.read_text())["gameState"]
    ps = gs["players"].values() if isinstance(gs["players"], dict) else gs["players"]
    names = [p.get("co_name", "?") for p in sorted(ps, key=lambda p: int(p.get("players_order", 0)))]
    return " vs ".join(names) if len(names) <= 2 else f"{len(names)} players"


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rows = []
    for src in ("backfill.jsonl", "history.jsonl"):
        p = ROOT / "metrics" / src
        for line in (p.read_text().splitlines() if p.exists() else []):
            r = json.loads(line)
            for corpus, v in r.get("replay", {}).items():
                if "turns" in v:
                    rows.append({"commit": r["commit"], "time": r["time"], "subject": r.get("subject", ""),
                                 "corpus": corpus, "turns": v["turns"], "clean": v["clean"]})
    seen, hist = set(), []
    for r in sorted(rows, key=lambda r: r["time"]):
        if (r["commit"], r["corpus"]) not in seen:
            seen.add((r["commit"], r["corpus"]))
            hist.append(r)
    write("history.csv", hist)

    checks, games = {}, []
    for corpus, dirs in metrics.corpora().items():
        for d in dirs:
            out = subprocess.run([str(metrics.BIN), d], capture_output=True, text=True).stdout
            m = re.search(r"turns compared: (\d+)\s+clean: (\d+)", out)
            for c in re.finditer(r"^(\S+)\s+(\d+) ok\s+(\d+) bad", out, re.M):
                a = checks.setdefault(c.group(1), [0, 0])
                a[0] += int(c.group(2))
                a[1] += int(c.group(2)) + int(c.group(3))
            days = len({re.search(r"-D(\d+)-", f.name).group(1) for f in Path(d).glob("turn-*-D*-*.json")})
            games.append({"game": Path(d).name.removeprefix("game-"), "corpus": corpus, "cos": cos_of(d),
                          "days": days, "turns": int(m.group(1)) if m else 0, "clean": int(m.group(2)) if m else 0})
    write("checks.csv", [{"check": k, "area": AREA.get(k.split(".")[0], k.split(".")[0]), "ok": v[0], "total": v[1]}
                         for k, v in sorted(checks.items())])
    write("games.csv", games)


def write(name, rows):
    with open(OUT / name, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0]))
        w.writeheader()
        w.writerows(rows)
    print(f"{name}: {len(rows)} rows")


if __name__ == "__main__":
    main()
