#!/usr/bin/env bash
# Politely fetch AWBW replays (one request per PAUSE seconds) for games listed in a JSON file
# of {"id","tier","days"}, into ~/repos/libaw-data/replays/game-<id>. Skips games
# already fetched. Uses awbw_chart's exporter.
#   tools/fetch_replays.sh LIST.json [TIERS=T1,T2] [MIN_DAYS=6] [MAX_GAMES=25]
set -euo pipefail
LIST=$1; TIERS=${2:-T1,T2}; MIN_DAYS=${3:-6}; MAX=${4:-25}; PAUSE=${PAUSE:-2}
OUT=~/repos/libaw-data/replays
PY=~/repos/awbw_chart/venv/bin/python
ids=$(python3 -I -c "
import json,sys
t=set(sys.argv[2].split(','))
g=[x for x in json.load(open(sys.argv[1])) if x['tier'] in t and (x['days'] or 0)>=int(sys.argv[3])]
print(' '.join(str(x['id']) for x in g[:int(sys.argv[4])]))" "$LIST" "$TIERS" "$MIN_DAYS" "$MAX")
for id in $ids; do
  d="$OUT/game-$id"
  if [ -f "$d/.done" ]; then continue; fi
  echo "== $id"
  # The exporter can exit 0 after a network error (DNS outage, 2026-11-05: 27 empty games
  # marked done): require turn files too.
  if "$PY" ~/repos/awbw_chart/awbw_replay_export.py --game-id "$id" --out-dir "$d" --pause "$PAUSE" >"$d.log" 2>&1 \
     && ls "$d"/turn-*.json >/dev/null 2>&1; then
    touch "$d/.done"; echo "ok $(ls "$d" | wc -l) turns"
  else
    echo "failed (see $d.log)"
  fi
done
