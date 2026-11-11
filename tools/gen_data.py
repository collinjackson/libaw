#!/usr/bin/env python3
"""Generate the engine's data tables (data/*.json) from their sources.

Sources (see docs/sources.md):
  data/replay-player/{Tiles,Buildings,Units}.json   AWBW-Replay-Player, MIT
  AWBW damage chart page (awbw.amarriner.com/damage.php), saved HTML, path in argv[1]

  python3 -I tools/gen_data.py path/to/damage.html
"""
import json, re, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "data" / "replay-player"
MOVE = {"LightInf": "F", "HeavyInf": "B", "Tread": "T", "Tire": "W", "Air": "A", "Sea": "S",
        "Lander": "L", "Pipe": "P"}


def load(p):
    s = Path(p).read_text()
    s = re.sub(r"/\*.*?\*/", "", s, flags=re.S)
    s = re.sub(r"//[^\n]*", "", s)
    s = re.sub(r",(\s*[}\]])", r"\1", s)
    return json.loads(s)


def terrain():
    out = {}
    for name, t in load(SRC / "Tiles.json").items():
        out[t["AWBWID"]] = {"name": name, "kind": t["TerrainType"], "defense": t["BaseDefence"],
                            "move": {MOVE[k]: v for k, v in t["MovementCostsPerType"].items()}}
    for name, b in load(SRC / "Buildings.json").items():
        out[b["AWBWID"]] = {"name": b.get("Name", name), "kind": "Building",
                            "building": b.get("BuildingType"), "defense": b["BaseDefence"],
                            "move": {MOVE[k]: v for k, v in b["MovementCostsPerType"].items()}}
    return {str(k): out[k] for k in sorted(out)}


def cos():
    """Each CO's day-to-day (always-on) ability, as the replay player's data has it."""
    out = {}
    for name, c in load(SRC / "COs.json").items():
        out[name] = {"id": c.get("AWBWID"), "d2d": c.get("DayToDayPower") or {},
                     "cop": c.get("NormalPower") or {}, "scop": c.get("SuperPower") or {}}
    return awbw_corrections(out)


def awbw_corrections(cos):
    """Where the replay player's table differs from AWBW.
    It lists Missile twice where Rocket belongs (Max's and Sensei's weakened units): AWBW's
    Rockets get the same -10% attack, and Max's -1 range. And Jake's powers lengthen every
    indirect's range except Battleships and Carriers, so Piperunners too."""
    # Drake since 2024-08: naval units defend at +10% (the table says +25%) and air units
    # attack at -30% (the table says -20%). data/cos.json already had these by hand.
    for e in cos.get("Drake", {}).get("d2d", {}).get("PowerIncreases") or []:
        if e.get("DefenseIncrease") == 0.25:
            e["DefenseIncrease"] = 0.1
        if e.get("PowerIncrease") == -0.2:
            e["PowerIncrease"] = -0.3
    for part in ("cop", "scop"):
        for e in cos.get("Jake", {}).get(part, {}).get("PowerIncreases") or []:
            if e.get("RangeIncrease") and "Piperunner" not in e["AffectedUnits"]:
                e["AffectedUnits"].append("Piperunner")
    for co in cos.values():
        for part in ("d2d", "cop", "scop"):
            for e in co[part].get("PowerIncreases") or []:
                au = e.get("AffectedUnits") or []
                if au.count("Missile") == 2 and "Rocket" not in au:
                    au[len(au) - 1 - au[::-1].index("Missile")] = "Rocket"
    return cos


def units():
    out = {}
    for name, u in load(SRC / "Units.json").items():
        out[name] = {"id": u["AWBWId"], "cost": u["Cost"], "move": u["MovementRange"],
                     "move_type": MOVE[u["MovementType"]], "fuel": u["MaxFuel"],
                     "fuel_per_turn": u["FuelUsagePerTurn"], "ammo": u["MaxAmmo"],
                     "vision": u["Vision"], "range": [u["AttackRange"]["x"], u["AttackRange"]["y"]],
                     "second_weapon": u["SecondWeapon"]}
    return out


DAMAGE_NAMES = {"anti-air": "Anti-Air", "apc": "APC", "artillery": "Artillery", "b-copter": "B-Copter",
    "battleship": "Battleship", "blackboat": "Black Boat", "blackbomb": "Black Bomb", "bomber": "Bomber",
    "carrier": "Carrier", "cruiser": "Cruiser", "fighter": "Fighter", "infantry": "Infantry",
    "lander": "Lander", "md.tank": "Md.Tank", "mech": "Mech", "megatank": "Mega Tank",
    "missile": "Missile", "neotank": "Neotank", "piperunner": "Piperunner", "recon": "Recon",
    "rocket": "Rocket", "stealth": "Stealth", "sub": "Sub", "t-copter": "T-Copter", "tank": "Tank"}


def damage(html_path):
    s = Path(html_path).read_text(errors="replace")
    t = s[s.find('id="tablehighlight"'):]
    t = t[:t.find("</table>")]
    out = {}
    for row in re.split(r'<tr id="highlights\d+">', t)[1:]:
        att = DAMAGE_NAMES[re.search(r"aw1/ge([a-z.\-]+?)\.gif", row).group(1)]
        cells = re.findall(r"<td id=yc([a-z.\-]+)_\d+[^>]*>(.*?)</td>", row, re.S)
        out[att] = {DAMAGE_NAMES[d]: int(v) for d, v in
                    ((d, re.sub(r"<[^>]+>", "", v).strip()) for d, v in cells) if v.isdigit()}
    return out


def main():
    data = ROOT / "data"
    (data / "terrain.json").write_text(json.dumps(terrain(), indent=1) + "\n")
    (data / "units.json").write_text(json.dumps(units(), indent=1) + "\n")
    (data / "cos.json").write_text(json.dumps(cos(), indent=1) + "\n")
    if len(sys.argv) > 1:
        (data / "damage.json").write_text(json.dumps(damage(sys.argv[1]), indent=1) + "\n")
    print("wrote", ", ".join(p.name for p in sorted(data.glob("*.json"))))


if __name__ == "__main__":
    main()
