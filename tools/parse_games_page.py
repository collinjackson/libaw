#!/usr/bin/env python3
"""Parse saved AWBW gamescompleted.php pages into [{"id","tier","days","cos"}].
  python3 -I tools/parse_games_page.py page1.html [page2.html ...] > list.json"""
import json, re, sys

out, seen = [], set()
for path in sys.argv[1:]:
    s = open(path, errors="replace").read()
    for block in s.split('<a class="anchor" name="game_')[1:]:
        gid = int(re.match(r"(\d+)", block).group(1))
        if gid in seen:
            continue
        seen.add(gid)
        tier = re.search(r"\[(T\d)\]", block[:2000])
        day = re.search(r"<b>Day (\d+)</b>", block)
        cos = re.findall(r"class='co_portrait' src=terrain/co-portraits/\w+/(?:gs_)?([a-z]+)\.png", block)
        out.append({"id": gid, "tier": tier.group(1) if tier else None,
                    "days": int(day.group(1)) if day else None, "cos": sorted(set(cos))})
json.dump(out, sys.stdout, indent=0)
