#!/usr/bin/env python3
"""Draw libaw's placeholder art: original SVG sprites, written by code (no image model, no AW art).

    python3 -I web/assets/make_sprites.py

Writes, next to this script:
  sprites.svg        one sheet of <symbol>s (terrain, buildings, units, overlays) for the web client
  svg/terrain/*.svg  each terrain piece as a standalone file
  svg/units/*.svg    each unit as a standalone file
  terrain-map.json   AWBW terrain id -> how to draw it (base, arms, building), from data/terrain.json

Every sprite is a 32x32 viewBox, flat shapes with a dark outline. Team colours come from CSS
custom properties, so one sprite serves every army:
  --team        the army colour            (fallback orange)
  --team-dark   its shade, for outlines and treads
  --team-light  its tint, for highlights
Buildings use the same variables (a neutral building gets a grey set). Units face right; the
client mirrors them for the second army if it wants.
"""
import json, re
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
INK = "var(--ink, #1f2430)"
T, TD, TL = "var(--team, #e5603b)", "var(--team-dark, #9a3416)", "var(--team-light, #ffc2ad)"

# ---- terrain -------------------------------------------------------------------------
GRASS, GRASS2 = "#9fd37c", "#86c264"
WATER, WATER2 = "#5aa9e6", "#3f8fd2"
SAND = "#f1d79a"
ROCK, ROCK2 = "#a3927e", "#7f6f5c"


def plain():
    return (f'<rect width="32" height="32" fill="{GRASS}"/>'
            f'<path d="M6 9l1-3 1 3M22 6l1-3 1 3M14 22l1-3 1 3M25 25l1-3 1 3M5 27l1-3 1 3" '
            f'stroke="{GRASS2}" stroke-width="1.2" fill="none" stroke-linecap="round"/>')


def wood():
    tree = lambda x, y, r: (f'<circle cx="{x}" cy="{y + 1.5}" r="{r}" fill="#2f6b3a" opacity=".35"/>'
                            f'<circle cx="{x}" cy="{y}" r="{r}" fill="#3f8f4a" stroke="#2a5e33" stroke-width="1"/>'
                            f'<circle cx="{x - r / 3}" cy="{y - r / 3}" r="{r / 3}" fill="#62b26a"/>')
    return plain() + tree(10, 11, 6.5) + tree(22, 12, 6) + tree(16, 22, 7)


def mountain():
    return (plain() + f'<path d="M2 28 L12 8 L19 20 L22 14 L30 28 Z" fill="{ROCK}" stroke="{ROCK2}" stroke-width="1.2" stroke-linejoin="round"/>'
            f'<path d="M12 8 L9 14 L12 13 L14 15 L15.5 12.8 Z" fill="#fbfbf7"/>'
            f'<path d="M22 14 L20.3 17.5 L22.5 17 L23.8 17.8 Z" fill="#fbfbf7"/>'
            f'<path d="M12 8 L15 28" stroke="{ROCK2}" stroke-width="1" opacity=".6"/>')


def sea():
    return (f'<rect width="32" height="32" fill="{WATER}"/>'
            f'<path d="M4 10q3-3 6 0t6 0M16 22q3-3 6 0t6 0M3 26q2.5-2.5 5 0" stroke="#bfe3ff" stroke-width="1.3" fill="none" stroke-linecap="round"/>')


def reef():
    return (sea() + f'<path d="M7 20l3-6 4 3 2-4 3 5-3 4-6 1z" fill="{ROCK}" stroke="{ROCK2}" stroke-width="1"/>'
            f'<path d="M20 11l2-3 3 2-1 3-3 1z" fill="{ROCK}" stroke="{ROCK2}" stroke-width="1"/>')


def shoal():
    return (f'<rect width="32" height="32" fill="{SAND}"/>'
            f'<path d="M7 9q2-1.5 4 0M19 20q2-1.5 4 0M10 24q2-1.5 4 0" stroke="#d9bd7c" stroke-width="1.1" fill="none" stroke-linecap="round"/>')


def shoal_edges():
    """Water lapping one side of a shoal; the client adds one per side that touches sea."""
    edge = (f'<path d="M0 0H32V7Q28 10 24 7T16 7T8 7T0 7Z" fill="{WATER}"/>'
            f'<path d="M0 7Q4 10 8 7T16 7T24 7T32 7" stroke="#fff6dd" stroke-width="1.4" fill="none"/>')
    return {f"shoal-{side}": f'<g transform="rotate({deg} 16 16)">{edge}</g>' for side, deg in (("n", 0), ("e", 90), ("s", 180), ("w", 270))}


def arms(kind):
    """Road / river / pipe pieces: a centre plus one arm per side, joined by the client."""
    st = {"road": ("#b9b6ad", "#8f8b80", 12), "river": (WATER, WATER2, 12), "pipe": ("#9aa6b5", "#5d6b7d", 10)}[kind]
    fill, edge, w = st
    h = (32 - w) / 2
    out = {f"{kind}-c": f'<rect x="{h}" y="{h}" width="{w}" height="{w}" fill="{fill}"/>'}
    for side, (x, y, ww, hh) in {"n": (h, 0, w, h + 0.5), "s": (h, 32 - h - 0.5, w, h + 0.5),
                                 "w": (0, h, h + 0.5, w), "e": (32 - h - 0.5, h, h + 0.5, w)}.items():
        stripe = ""
        if kind == "road":
            stripe = (f'<line x1="16" y1="{y + 1}" x2="16" y2="{y + hh - 1}" stroke="#f5f1e3" stroke-width="1.2" stroke-dasharray="2.5 2"/>'
                      if side in "ns" else
                      f'<line x1="{x + 1}" y1="16" x2="{x + ww - 1}" y2="16" stroke="#f5f1e3" stroke-width="1.2" stroke-dasharray="2.5 2"/>')
        if kind == "pipe":
            stripe = (f'<line x1="{h + 2}" y1="{y}" x2="{h + 2}" y2="{y + hh}" stroke="#c8d1dc" stroke-width="1.5"/>'
                      if side in "ns" else f'<line x1="{x}" y1="{h + 2}" x2="{x + ww}" y2="{h + 2}" stroke="#c8d1dc" stroke-width="1.5"/>')
        out[f"{kind}-{side}"] = f'<rect x="{x}" y="{y}" width="{ww}" height="{hh}" fill="{fill}"/>{stripe}'
    out[f"{kind}-c"] += (f'<rect x="{h}" y="{h}" width="{w}" height="{w}" fill="none" stroke="{edge}" stroke-width=".8" opacity=".5"/>'
                         if kind == "pipe" else "")
    return out


def bridge(vertical):
    planks = "".join(f'<rect x="{"5" if vertical else str(3 + i * 4.3)}" y="{str(3 + i * 4.3) if vertical else "9"}" '
                     f'width="{"22" if vertical else "3.2"}" height="{"3.2" if vertical else "14"}" rx=".6" fill="#b88a5a"/>' for i in range(6))
    rails = ('<path d="M5 0V32M27 0V32" stroke="#7a5532" stroke-width="2"/>' if vertical
             else '<path d="M0 9H32M0 23H32" stroke="#7a5532" stroke-width="2"/>')
    return sea() + planks + rails


def seam():
    return plain() + arms("pipe")["pipe-c"] + '<path d="M11 13l10 6M11 19l10-6" stroke="#5d6b7d" stroke-width="1.5"/>'


def rubble():
    return plain() + '<path d="M7 22l4-6 5 2 3-5 6 4 1 6z" fill="#8a96a6" stroke="#5d6b7d" stroke-width="1"/>'


def silo(loaded):
    rocket = ('<path d="M16 5c3 3 3 9 3 13h-6c0-4 0-10 3-13z" fill="#e8e3d5" stroke="#5b5348" stroke-width="1"/>'
              '<path d="M13 18l-2 3h10l-2-3z" fill="#c45040"/>') if loaded else ""
    return (plain() + '<ellipse cx="16" cy="24" rx="11" ry="4.5" fill="#8a8f98" stroke="#555b66" stroke-width="1"/>'
            '<ellipse cx="16" cy="23" rx="7" ry="2.6" fill="#3b404a"/>' + rocket)


# ---- buildings (drawn over plain; team colours) ---------------------------------------
def roof(x, y, w, h):
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="1.2" fill="{T}" stroke="{TD}" stroke-width="1.2"/>'


def windows(xs, ys):
    return "".join(f'<rect x="{x}" y="{y}" width="2.4" height="2.4" fill="{TL}"/>' for x in xs for y in ys)


def flag(x, y):
    return (f'<path d="M{x} {y}V{y - 9}" stroke="{INK}" stroke-width="1.2"/>'
            f'<path d="M{x} {y - 9}h6l-1.5 2 1.5 2h-6z" fill="{T}" stroke="{TD}" stroke-width=".8"/>')


def building(kind):
    shadow = '<ellipse cx="16" cy="28.5" rx="12" ry="2.2" fill="#000" opacity=".15"/>'
    if kind == "City":
        body = roof(5, 13, 10, 15) + roof(15, 8, 12, 20) + windows([7.5, 10.8], [16, 20, 24]) + windows([18, 21.5], [11, 15, 19, 23])
    elif kind == "Base":
        body = (roof(4, 14, 24, 14) + f'<path d="M4 14l12-7 12 7z" fill="{TD}"/>'
                f'<rect x="12" y="19" width="8" height="9" fill="{INK}" opacity=".75"/>'
                f'<path d="M12 21.5h8M12 24h8M12 26.5h8" stroke="{TL}" stroke-width=".8"/>')
    elif kind == "Airport":
        body = (roof(3, 18, 26, 10) + f'<path d="M3 18q13-9 26 0" fill="{TD}"/>'
                f'<path d="M23 12V5" stroke="{INK}" stroke-width="1.2"/><path d="M23 5l5 2-5 2z" fill="{TL}" stroke="{TD}" stroke-width=".7"/>'
                f'<rect x="7" y="21" width="18" height="2" fill="{TL}"/>')
    elif kind == "Port":
        body = (f'<rect x="0" y="22" width="32" height="10" fill="{WATER}"/>' + roof(4, 12, 15, 12) + windows([7, 10.5, 14], [15, 19])
                + f'<path d="M23 23V7M23 8l6 3" stroke="{INK}" stroke-width="1.4" fill="none"/>'
                f'<path d="M28.5 11v4" stroke="{INK}" stroke-width=".8"/><rect x="27" y="15" width="3" height="2.5" fill="{T}"/>')
    elif kind == "HQ":
        body = (roof(4, 14, 24, 14) + f'<path d="M4 14l12-9 12 9z" fill="{TD}"/>'
                f'<circle cx="16" cy="12" r="2.6" fill="{TL}"/>' + windows([7.5, 22], [18, 23])
                + f'<rect x="13" y="19" width="6" height="9" rx="3" fill="{INK}" opacity=".75"/>' + flag(16, 5.5))
    elif kind == "Com Tower":
        body = (roof(9, 20, 14, 8) + f'<path d="M16 20V5M11 20l5-10 5 10" stroke="{TD}" stroke-width="1.6" fill="none"/>'
                f'<circle cx="16" cy="5" r="2" fill="{T}" stroke="{TD}" stroke-width="1"/>'
                f'<path d="M11 4.5q-2 .5-2 3M21 4.5q2 .5 2 3M8.5 2.5q-3 1.5-3 5.5M23.5 2.5q3 1.5 3 5.5" stroke="{T}" stroke-width="1.1" fill="none"/>')
    elif kind == "Lab":
        body = (roof(5, 16, 22, 12) + f'<path d="M5 16q11-12 22 0z" fill="{TL}" stroke="{TD}" stroke-width="1.2"/>'
                f'<path d="M14 9h4M15 9v4l-3 4h8l-3-4V9" stroke="{INK}" stroke-width="1" fill="{T}"/>' + windows([8, 21.5], [20, 24]))
    else:
        raise ValueError(kind)
    return shadow + body


# ---- units -----------------------------------------------------------------------------
def treads(x, y, w, h=5):
    wheels = "".join(f'<circle cx="{x + 2.5 + i * (w - 5) / 3:.2f}" cy="{y + h / 2}" r="1.3" fill="{TL}"/>' for i in range(4))
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{h / 2}" fill="{INK}"/>' + wheels


def wheels(xs, y, r=2.6):
    return "".join(f'<circle cx="{x}" cy="{y}" r="{r}" fill="{INK}"/><circle cx="{x}" cy="{y}" r="{r / 2.3}" fill="#c7ccd6"/>' for x in xs)


def hull(x, y, w, h, rx=2):
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{T}" stroke="{INK}" stroke-width="1.2"/>'


def barrel(x1, y1, x2, y2, w=2.2):
    return f'<path d="M{x1} {y1}L{x2} {y2}" stroke="{INK}" stroke-width="{w + 1.4}" stroke-linecap="round"/><path d="M{x1} {y1}L{x2} {y2}" stroke="{TD}" stroke-width="{w}" stroke-linecap="round"/>'


def turret(cx, cy, r):
    return f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="{T}" stroke="{INK}" stroke-width="1.2"/><circle cx="{cx - r / 3}" cy="{cy - r / 3}" r="{r / 3}" fill="{TL}"/>'


def person(x, weapon=None):
    body = (f'<circle cx="{x}" cy="9" r="3.4" fill="{T}" stroke="{INK}" stroke-width="1.1"/>'
            f'<path d="M{x - 4.2} 9.2h8.4" stroke="{TD}" stroke-width="1.6" stroke-linecap="round"/>'
            f'<path d="M{x - 4} 25V16q0-3.5 4-3.5t4 3.5V25z" fill="{T}" stroke="{INK}" stroke-width="1.1"/>'
            f'<path d="M{x - 2.5} 25v4M{x + 2.5} 25v4" stroke="{INK}" stroke-width="2.4" stroke-linecap="round"/>')
    if weapon == "rifle":
        body += f'<path d="M{x - 1} 18l9-3" stroke="{INK}" stroke-width="1.8" stroke-linecap="round"/>'
    if weapon == "bazooka":
        body += (f'<path d="M{x - 6} 14.5l15-2.5" stroke="{INK}" stroke-width="4.2" stroke-linecap="round"/>'
                 f'<path d="M{x - 6} 14.5l15-2.5" stroke="{TD}" stroke-width="2.4" stroke-linecap="round"/>')
    return body


def rotor(cx, y, span):
    return (f'<path d="M{cx} {y}v3" stroke="{INK}" stroke-width="1.6"/>'
            f'<ellipse cx="{cx}" cy="{y}" rx="{span}" ry="1.4" fill="{INK}" opacity=".75"/>')


def ship(x, y, w, h):
    return f'<path d="M{x} {y}H{x + w}L{x + w - 4} {y + h}H{x + 3}Z" fill="{T}" stroke="{INK}" stroke-width="1.2" stroke-linejoin="round"/>'


UNITS = {
    "Infantry": lambda: person(13, "rifle"),
    "Mech": lambda: person(12, "bazooka"),
    "Recon": lambda: hull(4, 13, 23, 9, 3) + f'<path d="M9 13l3-4h8l3 4z" fill="{TL}" stroke="{INK}" stroke-width="1"/>' + barrel(20, 11, 27, 11, 1.4) + wheels([9, 22], 23),
    "Tank": lambda: treads(4, 21, 24) + hull(6, 14, 20, 8) + turret(15, 14, 4.5) + barrel(18, 13, 28, 13),
    "Md.Tank": lambda: treads(2, 21, 28, 6) + hull(4, 13, 24, 9) + turret(14, 13, 5.5) + barrel(18, 11.5, 30, 11.5, 2.6) + barrel(18, 15, 29, 15, 1.6),
    "Neotank": lambda: treads(3, 21, 26, 6) + f'<path d="M5 22q0-10 11-10t11 10z" fill="{T}" stroke="{INK}" stroke-width="1.2"/>' + turret(15, 13, 4.5) + barrel(18, 11.5, 29, 11.5, 1.8) + barrel(18, 14.5, 29, 14.5, 1.8),
    "Mega Tank": lambda: treads(1, 22, 30, 7) + hull(2, 13, 28, 10) + turret(10, 12, 4.2) + turret(19, 12, 4.2) + barrel(12, 10, 28, 7, 2.2) + barrel(21, 10.5, 31, 9, 2.2),
    "APC": lambda: treads(3, 21, 26) + f'<path d="M4 22V14l4-4h18l2 4v8z" fill="{T}" stroke="{INK}" stroke-width="1.2"/>' + f'<rect x="10" y="13" width="9" height="4" rx="1" fill="{TL}"/><path d="M22 13h3v4h-3z" fill="{INK}" opacity=".6"/>',
    "Artillery": lambda: treads(4, 21, 22) + hull(6, 15, 16, 7) + barrel(13, 15, 28, 6, 2.4) + f'<circle cx="13" cy="15" r="3" fill="{TD}" stroke="{INK}" stroke-width="1"/>',
    "Rocket": lambda: hull(3, 15, 26, 7, 1.5) + f'<rect x="21" y="10" width="7" height="6" rx="1" fill="{TL}" stroke="{INK}" stroke-width="1"/>' + f'<g transform="rotate(-25 11 13)"><rect x="4" y="9" width="15" height="7" rx="1" fill="{TD}" stroke="{INK}" stroke-width="1"/><circle cx="17" cy="11" r="1" fill="{TL}"/><circle cx="17" cy="14" r="1" fill="{TL}"/></g>' + wheels([7, 15, 25], 23, 2.4),
    "Missile": lambda: hull(3, 16, 26, 6, 1.5) + f'<rect x="22" y="11" width="6" height="6" rx="1" fill="{TL}" stroke="{INK}" stroke-width="1"/>' + "".join(f'<path d="M{x} 16V6l1.8-2.5L{x + 3.6} 6v10z" fill="#eef0f3" stroke="{INK}" stroke-width="1"/>' for x in (6, 11, 16)) + wheels([7, 15, 25], 23, 2.4),
    "Anti-Air": lambda: treads(4, 21, 24) + hull(6, 14, 20, 8) + turret(14, 14, 4) + barrel(16, 12, 27, 5, 1.5) + barrel(17, 14, 29, 8, 1.5),
    "Piperunner": lambda: f'<path d="M1 26H31" stroke="#5d6b7d" stroke-width="3"/>' + hull(3, 14, 26, 9, 2.5) + wheels([8, 16, 24], 24, 2) + f'<rect x="7" y="9" width="12" height="6" rx="1" fill="{TD}" stroke="{INK}" stroke-width="1"/>' + barrel(17, 11, 29, 6, 2),
    "Fighter": lambda: f'<path d="M30 16L6 9 8 16 6 23z" fill="{T}" stroke="{INK}" stroke-width="1.2" stroke-linejoin="round"/><path d="M14 16L5 4h4l11 11zM14 16L5 28h4l11-11z" fill="{TD}" stroke="{INK}" stroke-width="1"/><ellipse cx="22" cy="16" rx="3" ry="1.6" fill="{TL}"/>',
    "Bomber": lambda: f'<path d="M3 13h24q3 0 3 3t-3 3H3z" fill="{T}" stroke="{INK}" stroke-width="1.2"/><path d="M11 13l4-10h4l2 10zM11 19l4 10h4l2-10z" fill="{TD}" stroke="{INK}" stroke-width="1"/><path d="M3 13l-1-5h3l3 5z" fill="{TD}" stroke="{INK}" stroke-width=".8"/><circle cx="26" cy="16" r="1.5" fill="{TL}"/>',
    "Stealth": lambda: f'<path d="M30 16L4 4 10 16 4 28z" fill="{TD}" stroke="{INK}" stroke-width="1.2" stroke-linejoin="round"/><path d="M28 16L12 10.5 14 16 12 21.5z" fill="{T}"/><path d="M22 14.5l3 1.5-3 1.5z" fill="{TL}"/>',
    "Black Bomb": lambda: f'<circle cx="17" cy="17" r="8" fill="{INK}"/><circle cx="14.5" cy="14.5" r="2.4" fill="#5b6273"/><path d="M8 17H2M9 12L4 8M9 22l-5 4" stroke="{T}" stroke-width="2.4" stroke-linecap="round"/><path d="M23 11l3-4" stroke="{TD}" stroke-width="2"/><circle cx="26.5" cy="6.5" r="1.4" fill="#ffd25e"/>',
    "B-Copter": lambda: rotor(16, 6, 14) + f'<path d="M5 14h3l2 2h4" stroke="{INK}" stroke-width="2.2" fill="none"/>' + f'<path d="M12 13q0-4 7-4h3q6 0 6 6t-6 6h-6q-4 0-4-5z" fill="{T}" stroke="{INK}" stroke-width="1.2"/><path d="M21 11q4 0 5 4h-5z" fill="{TL}"/>' + barrel(20, 21, 27, 23, 1.3) + '<path d="M13 24h14" stroke="#1f2430" stroke-width="1.6"/>',
    "T-Copter": lambda: rotor(16, 5, 14) + f'<path d="M3 13h6" stroke="{INK}" stroke-width="2.4"/><path d="M8 12q0-4 6-4h8q7 0 7 7t-7 7H14q-6 0-6-6z" fill="{T}" stroke="{INK}" stroke-width="1.2"/><path d="M22 10q5 0 6 5h-6z" fill="{TL}"/><rect x="12" y="13" width="7" height="5" rx="1" fill="{TD}"/><path d="M11 25h15" stroke="#1f2430" stroke-width="1.6"/>',
    "Battleship": lambda: ship(1, 17, 30, 8) + turret(9, 15, 3.4) + turret(19, 15, 3.4) + barrel(11, 14, 18, 12, 1.4) + barrel(21, 14, 28, 12, 1.4) + f'<rect x="12" y="9" width="4" height="6" fill="{TD}" stroke="{INK}" stroke-width="1"/>',
    "Cruiser": lambda: ship(2, 18, 28, 7) + f'<rect x="10" y="11" width="10" height="7" rx="1" fill="{TD}" stroke="{INK}" stroke-width="1"/><path d="M15 11V5M12 6h6" stroke="{INK}" stroke-width="1.2"/>' + barrel(21, 16, 28, 14, 1.4) + f'<circle cx="23" cy="17" r="2" fill="{T}" stroke="{INK}"/>',
    "Sub": lambda: f'<path d="M2 21q0-6 9-6h11q8 0 8 6t-8 5H11q-9 0-9-5z" fill="{T}" stroke="{INK}" stroke-width="1.2"/><path d="M13 15v-5h6l1 5z" fill="{TD}" stroke="{INK}" stroke-width="1"/><path d="M16 10V6" stroke="{INK}" stroke-width="1.2"/><circle cx="24" cy="20" r="1.3" fill="{TL}"/><circle cx="8" cy="20" r="1.3" fill="{TL}"/>',
    "Lander": lambda: ship(1, 15, 30, 10) + f'<rect x="6" y="10" width="15" height="6" rx="1" fill="{TL}" stroke="{INK}" stroke-width="1"/><path d="M23 15l6-6" stroke="{INK}" stroke-width="2"/><rect x="22" y="9" width="5" height="6" fill="{TD}" stroke="{INK}" stroke-width="1"/>',
    "Black Boat": lambda: ship(4, 18, 24, 6) + f'<rect x="10" y="13" width="9" height="5" rx="1" fill="{TD}" stroke="{INK}" stroke-width="1"/><path d="M21 18l3-6h3" stroke="{INK}" stroke-width="1.5" fill="none"/><path d="M12 15.5h5M14.5 13v5" stroke="{TL}" stroke-width="1.2"/>',
    "Carrier": lambda: ship(1, 15, 30, 10) + f'<rect x="3" y="13" width="26" height="3" fill="{TD}" stroke="{INK}" stroke-width="1"/><path d="M5 14.5h20" stroke="{TL}" stroke-width=".8" stroke-dasharray="2 2"/><rect x="21" y="7" width="5" height="6" fill="{T}" stroke="{INK}" stroke-width="1"/>',
}

OVERLAYS = {
    # a fogged tile: the client lays this over terrain it can't see
    "fog": '<rect width="32" height="32" fill="#1b2233" opacity=".42"/>',
    "cursor": f'<rect x="1.5" y="1.5" width="29" height="29" rx="4" fill="none" stroke="#fff" stroke-width="2.4"/><rect x="1.5" y="1.5" width="29" height="29" rx="4" fill="none" stroke="{INK}" stroke-width="1" stroke-dasharray="4 3"/>',
    "capture": f'<rect x="20" y="1" width="11" height="9" rx="2" fill="{INK}"/><path d="M23 8V3l5 1.5L23 6" fill="#ffd25e"/>',
}

BUILDINGS = ["City", "Base", "Airport", "Port", "HQ", "Com Tower", "Lab"]


def terrain_pieces():
    p = {"plain": plain(), "wood": wood(), "mountain": mountain(), "sea": sea(), "reef": reef(), "shoal": shoal(),
         "bridge-h": bridge(False), "bridge-v": bridge(True), "seam": seam(), "rubble": rubble(),
         "silo": silo(True), "silo-empty": silo(False)}
    for k in ("road", "river", "pipe"):
        p.update(arms(k))
    p.update(shoal_edges())
    for b in BUILDINGS:
        p["bld-" + slug(b)] = building(b)
    return p


def slug(name):
    return re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")


def terrain_map():
    """AWBW terrain id -> {"base": piece, "arms": "nesw" subset, "kind": road/river/pipe, "building": name,
    "shoal": side}. Arms come from the name's direction letters (HRoad = e+w, ESRoad = e+s, CRiver = all)."""
    out = {}
    for tid, t in json.load(open(ROOT / "data/terrain.json")).items():
        name, b = t["name"], t.get("building")
        d = {"name": name}
        if b == "Missile":
            d["base"] = "silo-empty" if "Empty" in name else "silo"
        elif b == "PipeSeam":
            d["base"] = "seam"
        elif b:
            d["base"] = "plain"
            d["building"] = slug("Com Tower" if b in ("Com Tower", "ComTower") else b)
        elif name == "Plain":
            d["base"] = "plain"
        elif name == "Mountain":
            d["base"] = "mountain"
        elif name == "Wood":
            d["base"] = "wood"
        elif name == "Sea":
            d["base"] = "sea"
        elif name == "Reef":
            d["base"] = "reef"
        elif name.startswith("Shoal"):
            d["base"] = "shoal"   # water edges are drawn by the client, from the neighbouring tiles
        elif name.endswith("Bridge"):
            d["base"] = "bridge-h" if name.startswith("H") else "bridge-v"
        elif name == "Pipe Seam":
            d["base"] = "seam"
        elif "Rubble" in name:
            d["base"] = "rubble"
        elif name.startswith("Missile Silo"):
            d["base"] = "silo-empty" if "Empty" in name else "silo"
        else:
            m = re.match(r"([NESWHVC]+)(Road|River|Pipe)( End)?$", name)
            if m:
                letters, kind = m[1], m[2].lower()
                arms_ = {"H": "ew", "V": "ns", "C": "nesw"}.get(letters, letters.lower())
                d.update(base="plain", kind=kind, arms="".join(sorted(set(arms_))))
            else:
                d["base"] = "plain"
        out[tid] = d
    return out


def standalone(inner):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="64" height="64">{inner}</svg>\n'


def main():
    pieces = terrain_pieces()
    syms = [f'<symbol id="t-{k}" viewBox="0 0 32 32">{v}</symbol>' for k, v in pieces.items()]
    syms += [f'<symbol id="u-{slug(k)}" viewBox="0 0 32 32">{f()}</symbol>' for k, f in UNITS.items()]
    syms += [f'<symbol id="o-{k}" viewBox="0 0 32 32">{v}</symbol>' for k, v in OVERLAYS.items()]
    (HERE / "sprites.svg").write_text('<svg xmlns="http://www.w3.org/2000/svg" style="display:none">\n'
                                      + "\n".join(syms) + "\n</svg>\n")
    for sub, items in (("terrain", pieces.items()), ("units", ((slug(k), f()) for k, f in UNITS.items()))):
        d = HERE / "svg" / sub
        d.mkdir(parents=True, exist_ok=True)
        for k, v in items:
            ground = sub == "units" or k.startswith("bld-")   # units and buildings stand on plain
            (d / f"{k}.svg").write_text(standalone((plain() if ground else "") + v))
    units = json.load(open(ROOT / "data/units.json"))
    missing = [u for u in units if u not in UNITS]
    if missing:
        raise SystemExit(f"no sprite for {missing}")
    (HERE / "terrain-map.json").write_text(json.dumps(terrain_map(), separators=(",", ":")) + "\n")
    print(f"{len(pieces)} terrain pieces, {len(UNITS)} units, {len(OVERLAYS)} overlays")


if __name__ == "__main__":
    main()
