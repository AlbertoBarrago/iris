#!/usr/bin/env python3
"""Draws Iris's winged envelope: the seven Add Account poses and the app icons.

The envelope geometry per pose matches `Env::of` in app/src/ui/post_band.rs,
so the stamp, badge, dots and glint the app draws on top land in place. Change
the two together. Run from the repository root: scripts/iris-art.py .
"""
import math
import os
import sys

ROOT = sys.argv[1]
BAND = os.path.join(ROOT, "app/data/band")
ICONS = os.path.join(ROOT, "app/data/icons")

PAPER = "#fbf7f0"
# Degrees between neighbouring feathers.
SPREAD = 9
FRAME_TOP, FRAME_BOT = "#33267a", "#1d1547"
LIP_TOP, LIP_BOT = "#140f33", "#0e0a24"
FLAP = "#241a5c"
WING = [("0", "#5ad1e8"), ("0.32", "#8b6cf0"), ("0.68", "#f07ab8"), ("1", "#f5c46b")]

# (x, top, w, wing lift in degrees), as in post_band.rs.
POSES = {
    "idle": (60.0, 66.0, 124.0, 14.0),
    "stamped": (150.0, 62.0, 140.0, 10.0),
    "error": (150.0, 62.0, 140.0, 2.0),
    "browser": (52.0, 66.0, 124.0, 20.0),
    "lookup": (52.0, 66.0, 124.0, 20.0),
    "unreachable": (52.0, 66.0, 124.0, -8.0),
    "success": (154.0, 92.0, 132.0, 34.0),
}


def f(v):
    return f"{v:.2f}".rstrip("0").rstrip(".")


def defs(extra=""):
    stops = "".join(f'<stop offset="{o}" stop-color="{c}"/>' for o, c in WING)
    return (
        "<defs>"
        f'<linearGradient id="frame" x1="0" x2="0" y1="0" y2="1"><stop offset="0" stop-color="{FRAME_TOP}"/><stop offset="1" stop-color="{FRAME_BOT}"/></linearGradient>'
        f'<linearGradient id="lip" x1="0" x2="0" y1="0" y2="1"><stop offset="0" stop-color="{LIP_TOP}"/><stop offset="1" stop-color="{LIP_BOT}"/></linearGradient>'
        '<linearGradient id="paper" x1="0" x2="0" y1="0" y2="1"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#f6f2fb"/></linearGradient>'
        f'<linearGradient id="wing" x1="0" x2="1" y1="0" y2="0">{stops}</linearGradient>'
        '<filter id="shade" x="-30%" y="-30%" width="160%" height="170%"><feDropShadow dx="0" dy="3" stdDeviation="4" flood-color="#0e0a24" flood-opacity="0.32"/></filter>'
        f"{extra}</defs>"
    )


def rrect(x, y, w, h, r):
    return (
        f"M{f(x + r)} {f(y)}H{f(x + w - r)}A{f(r)} {f(r)} 0 0 1 {f(x + w)} {f(y + r)}"
        f"V{f(y + h - r)}A{f(r)} {f(r)} 0 0 1 {f(x + w - r)} {f(y + h)}"
        f"H{f(x + r)}A{f(r)} {f(r)} 0 0 1 {f(x)} {f(y + h - r)}V{f(y + r)}"
        f"A{f(r)} {f(r)} 0 0 1 {f(x + r)} {f(y)}Z"
    )


def wing(ax, ay, w, lift):
    """Three feathers fanned up and to the left from (ax, ay), longest last
    drawn first so the shorter ones sit in front."""
    out = []
    feathers = [(0.40, 0.12), (0.50, 0.13), (0.60, 0.135), (0.69, 0.13), (0.76, 0.12)]
    for i, (length, width) in reversed(list(enumerate(feathers))):
        L, W = length * w, width * w
        angle = 180 + lift + SPREAD * i
        leaf = (
            f"M0 0C{f(L * 0.25)} {f(-W)} {f(L * 0.8)} {f(-W * 0.8)} {f(L)} 0"
            f"C{f(L * 0.75)} {f(W * 0.45)} {f(L * 0.3)} {f(W * 0.5)} 0 0Z"
        )
        spine = f"M{f(L * 0.06)} 0Q{f(L * 0.5)} {f(-W * 0.22)} {f(L * 0.9)} 0"
        out.append(
            f'<g transform="translate({f(ax)} {f(ay)}) rotate({f(angle)})">'
            f'<path d="{leaf}" fill="url(#wing)" filter="url(#shade)"/>'
            f'<path d="{spine}" fill="none" stroke="#ffffff" stroke-opacity="0.45" stroke-width="{f(w * 0.012)}" stroke-linecap="round"/>'
            "</g>"
        )
    return "".join(out)


def wing_anchor(x, top, w):
    h = w * 78 / 108
    return x + 0.12 * w, top + 0.18 * h


def envelope(x, top, w, lift, flap=True):
    h = w * 78 / 108
    r = w * 0.11
    ax, ay = wing_anchor(x, top, w)
    px0, py0 = x + w * 0.074, top + w * 0.08
    px1, py1 = x + w * 0.926, top + h - w * 0.085
    pr = w * 0.052
    paper = (
        f"M{f(px0)} {f(py0)}H{f(px1)}V{f(py1 - pr)}A{f(pr)} {f(pr)} 0 0 1 {f(px1 - pr)} {f(py1)}"
        f"H{f(px0 + pr)}A{f(pr)} {f(pr)} 0 0 1 {f(px0)} {f(py1 - pr)}Z"
    )
    cx = x + w / 2
    parts = [
        wing(ax, ay, w, lift),
        f'<path d="{rrect(x, top + h * 0.09, w, h, r)}" fill="url(#lip)"/>',
        f'<path d="{rrect(x, top, w, h, r)}" fill="url(#frame)"/>',
        f'<path d="M{f(x + r)} {f(top + 0.8)}H{f(x + w - r)}" stroke="{PAPER}" stroke-opacity="0.18" stroke-width="1.2" stroke-linecap="round"/>',
        f'<path d="{paper}" fill="url(#paper)"/>',
        # The two lower folds, faint, so the paper reads as an envelope.
        f'<path d="M{f(px0 + 2)} {f(py1 - 2)}L{f(cx)} {f(top + h * 0.56)}L{f(px1 - 2)} {f(py1 - 2)}" fill="none" stroke="#c9c1e6" stroke-width="{f(w * 0.014)}" stroke-linejoin="round"/>',
    ]
    if flap:
        parts.append(f'<path d="M{f(px0 - 1)} {f(py0 - 1)}L{f(cx)} {f(top + h * 0.42)}L{f(px1 + 1)} {f(py0 - 1)}Z" fill="{FLAP}"/>')
        parts.append(f'<path d="M{f(px0 + 1)} {f(py0 + 0.5)}L{f(cx)} {f(top + h * 0.415)}L{f(px1 - 1)} {f(py0 + 0.5)}" fill="none" stroke="#8b6cf0" stroke-opacity="0.55" stroke-width="{f(w * 0.016)}" stroke-linejoin="round" stroke-linecap="round"/>')
    return "".join(parts)


def svg(body, extra_defs=""):
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" width="440" height="196" viewBox="0 0 440 196">'
        + defs(extra_defs) + body + "</svg>\n"
    )


DASH = f'fill="none" stroke="{PAPER}" stroke-width="2.2" stroke-linecap="round" stroke-dasharray="3 6"'


def server(y, lit):
    dot = "#3d9e6b" if lit else "#7b70b8"
    return (
        f'<rect x="306" y="{y}" width="58" height="18" rx="5" fill="{PAPER}" filter="url(#shade)"/>'
        f'<rect x="315" y="{y + 7.5}" width="22" height="3" rx="1.5" fill="#d0cbe8"/>'
        f'<circle cx="353" cy="{y + 9}" r="2.6" fill="{dot}"/>'
    )


def pose(name):
    x, top, w, lift = POSES[name]
    if name == "idle":
        word = (
            f'<text x="214" y="128" font-family="Adwaita Sans, SF Pro Display, Helvetica Neue, sans-serif" '
            f'font-size="64" font-weight="800" letter-spacing="-1.5" fill="{PAPER}">Iris</text>'
            f'<text x="217" y="152" font-family="Adwaita Sans, SF Pro Text, Helvetica Neue, sans-serif" '
            f'font-size="15" font-weight="600" fill="{PAPER}" fill-opacity="0.72">mail and calendar</text>'
        )
        return svg(envelope(x, top, w, lift) + word)
    if name in ("stamped", "error"):
        return svg(envelope(x, top, w, lift))
    if name == "browser":
        body = envelope(x, top, w, lift) + (
            f'<path d="M188 116Q240 68 286 104" {DASH} opacity="0.8"/>'
            f'<rect x="292" y="62" width="118" height="88" rx="10" fill="{PAPER}" filter="url(#shade)"/>'
            '<path d="M302 62H400A10 10 0 0 1 410 72V82H292V72A10 10 0 0 1 302 62Z" fill="#e4e0f5"/>'
            '<circle cx="305" cy="72" r="3" fill="#7b70b8"/><circle cx="315" cy="72" r="3" fill="#7b70b8"/><circle cx="325" cy="72" r="3" fill="#7b70b8"/>'
            '<circle cx="351" cy="100" r="9" fill="#d8d3ee"/>'
            '<rect x="321" y="115" width="60" height="5" rx="2.5" fill="#d8d3ee"/>'
            '<rect x="329" y="127" width="44" height="12" rx="6" fill="#6c4ce0"/>'
        )
        return svg(body)
    if name == "lookup":
        body = envelope(x, top, w, lift) + (
            f'<path d="M188 110Q246 70 300 70" {DASH} opacity="0.8"/>'
            f'<path d="M188 130Q246 162 300 136" {DASH} opacity="0.35"/>'
            f'<g>{server(50, True)}{server(72, True)}</g>'
            f'<g opacity="0.5">{server(116, False)}{server(138, False)}</g>'
        )
        return svg(body)
    if name == "unreachable":
        body = envelope(x, top, w, lift) + (
            f'<path d="M188 110Q231.5 80 245 80" {DASH} opacity="0.8"/>'
            f'<g opacity="0.5">{server(50, False)}{server(72, False)}</g>'
            f'<path transform="translate(257 74) scale(0.75)" d="M4.5 4.5L11.5 11.5M11.5 4.5L4.5 11.5" '
            f'stroke="{PAPER}" stroke-width="2.93" fill="none" stroke-linecap="round"/>'
        )
        return svg(body)
    if name == "success":
        h = w * 78 / 108
        cx = x + w / 2
        opened = (
            f'<path d="M{f(x + 10)} {f(top + 7)}L{f(cx)} {f(top - 52)}L{f(x + w - 10)} {f(top + 7)}Z" '
            f'fill="{FLAP}" stroke="{FLAP}" stroke-width="6.6" stroke-linejoin="round"/>'
            f'<rect x="{f(cx - 33)}" y="{f(top - 26)}" width="66" height="67" rx="5" fill="#ffffff" filter="url(#shade)"/>'
            f'<rect x="{f(cx + 2.6)}" y="{f(top - 18)}" width="19.8" height="4" rx="2" fill="#d8d3ee"/>'
            f'<rect x="{f(cx + 2.6)}" y="{f(top - 10)}" width="13.2" height="4" rx="2" fill="#d8d3ee"/>'
            f'<circle cx="{f(cx - 13.2)}" cy="{f(top - 12)}" r="10.56" fill="#1c7f4f"/>'
            f'<path d="M{f(cx - 18)} {f(top - 11.8)}L{f(cx - 14.3)} {f(top - 8)}L{f(cx - 8.1)} {f(top - 15.7)}" '
            'fill="none" stroke="#ffffff" stroke-width="2.75" stroke-linecap="round" stroke-linejoin="round"/>'
        )
        trail = f'<path d="M430 150Q420 50 264 62" fill="none" stroke="{PAPER}" stroke-width="2.2" stroke-linecap="round" stroke-dasharray="2 6" opacity="0.45"/>'
        return svg(opened + envelope(x, top, w, lift, flap=False) + trail)
    raise ValueError(name)


def icon(size_attr):
    """The app icon: a macOS squircle, night to violet, the winged envelope."""
    extra = (
        '<linearGradient id="bg" x1="0" x2="1" y1="0" y2="1"><stop offset="0" stop-color="#5b3fd1"/><stop offset="1" stop-color="#1b1340"/></linearGradient>'
        '<linearGradient id="sheen" x1="0" x2="0" y1="0" y2="1"><stop offset="0" stop-color="#ffffff" stop-opacity="0.22"/><stop offset="0.5" stop-color="#ffffff" stop-opacity="0"/></linearGradient>'
    )
    body = (
        f'<path d="{rrect(12, 12, 104, 104, 23.5)}" fill="url(#bg)"/>'
        f'<path d="{rrect(12, 12, 104, 104, 23.5)}" fill="url(#sheen)"/>'
        + envelope(40, 56, 60, 16)
    )
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" {size_attr} viewBox="0 0 128 128">'
        + defs(extra) + body + "</svg>\n"
    )


SYMBOLIC = """<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">
<g fill="#241f31">
<path d="M6.5 7H14a1.5 1.5 0 0 1 1.5 1.5v5A1.5 1.5 0 0 1 14 15H6.5A1.5 1.5 0 0 1 5 13.5v-5A1.5 1.5 0 0 1 6.5 7Zm0 1.3 3.75 2.9 3.75-2.9Z" fill-rule="evenodd"/>
<path d="M6 7.5C4.2 7 2.4 5.6 1.2 3.4 3.4 3.6 5.2 4.8 6.6 6.6Z"/>
<path d="M6.6 6.4C5.6 4.8 5 2.9 5.3 0.8 6.9 2.2 7.6 4 7.6 6Z"/>
</g>
</svg>
"""


def main():
    for name in POSES:
        with open(os.path.join(BAND, f"{name}.svg"), "w") as out:
            out.write(pose(name))
    app = "io.github.AlbertoBarrago.Iris"
    with open(os.path.join(ICONS, f"scalable/apps/{app}.svg"), "w") as out:
        out.write(icon('width="128" height="128"'))
    with open(os.path.join(ICONS, f"16x16/apps/{app}.svg"), "w") as out:
        out.write(icon('width="16" height="16"'))
    with open(os.path.join(ICONS, f"scalable/apps/{app}-symbolic.svg"), "w") as out:
        out.write(SYMBOLIC)
    # The glint's spot for post_band.rs: the longest feather's tip at rest.
    x, top, w, lift = POSES["idle"]
    ax, ay = wing_anchor(x, top, w)
    a = math.radians(lift + 4 * SPREAD)
    print(f"idle wing tip: ({ax - 0.76 * w * math.cos(a):.1f}, {ay - 0.76 * w * math.sin(a):.1f})")


main()
