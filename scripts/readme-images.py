"""Generates the README illustrations (SVG) in English and Spanish:

    python3 scripts/readme-images.py docs/images

The app icon embedded in api-*.svg is docs/images/app-icon.png (AppIcon.png at 220 px).
"""
import base64
import os
import sys

FONT = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif"
MONO = "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"

APP = {
    "Code": "#2f80ed",
    "Chrome": "#e8833a",
    "Firefox": "#e8833a",
    "Slack": "#8e44ad",
    "Teams": "#5b5fc7",
    "Mail": "#1fa463",
    "Terminal": "#334155",
    "Spotify": "#1db954",
    "Notes": "#d4a017",
    "Figma": "#e5484d",
}

T = {
    "en": {
        "s1": "Divide every display into a grid",
        "s2": "Assign each app to a rectangle",
        "s3": "It reopens right where it belongs",
        "cells": "8 × 4 cells",
        "open": "Laptop open",
        "closed": "Laptop closed",
        "detect": "Detects the connected displays",
        "same": "and switches layout on its own",
        "profile": "Profile “Home”",
        "stack": "Several windows in one zone: tabs",
        "dyn": "Apps without a rule: dynamic zone",
        "noRule": "no rule",
        "before": "Normal",
        "after": "Focus mode",
        "focusNote": "The active window grows to 80 %; the rest is dimmed",
        "deck": "Stream deck, macro pad or script",
        "token": "local HTTP + WebSocket, with a token",
        "app": "Venecita Dock",
        "cli": "or the venecita command",
        "rule": "rule",
        "switch": "switch tabs",
        "d1": "1 · Divide the display into a grid",
        "d2": "2 · Give each app a zone (the rest go to the dynamic zone)",
        "d3": "3 · Apps open right in their zone",
        "d4": "Two windows in one zone: tabs",
        "d5": "⌃⌥F · Focus mode: the window grows, the rest is dimmed",
        "d6": "Click another window: it goes back to its place",
        "dStatic": "Every app opens right in its zone",
        "dynamic": "dynamic",
        "demo": "How Venecita Dock works",
        "d7": "Open the laptop: the two-screen layout takes over",
        "d8": "Close it: everything goes back to the monitor, by itself",
        "d9": "At the office, two monitors: it switches to the Work profile by itself",
        "profileLbl": "Profile",
        "home": "Home",
        "work": "Work",
    },
    "es": {
        "s1": "Dividí cada pantalla en una grilla",
        "s2": "Asigná cada app a un rectángulo",
        "s3": "Al abrirse, vuelve a su lugar",
        "cells": "8 × 4 celdas",
        "open": "Notebook abierta",
        "closed": "Notebook cerrada",
        "detect": "Detecta las pantallas conectadas",
        "same": "y cambia de distribución sola",
        "profile": "Perfil “Casa”",
        "stack": "Varias ventanas en una zona: pestañas",
        "dyn": "Apps sin regla: zona dinámica",
        "noRule": "sin regla",
        "before": "Normal",
        "after": "Modo foco",
        "focusNote": "La ventana activa crece al 80 %; el resto se oscurece",
        "deck": "Stream deck, macro pad o script",
        "token": "HTTP + WebSocket local, con token",
        "app": "Venecita Dock",
        "cli": "o el comando venecita",
        "rule": "regla",
        "switch": "cambiar de pestaña",
        "d1": "1 · Dividí la pantalla en una grilla",
        "d2": "2 · Dale una zona a cada app (las demás van a la zona dinámica)",
        "d3": "3 · Las apps se abren en su zona",
        "d4": "Dos ventanas en una zona: pestañas",
        "d5": "⌃⌥F · Modo foco: la ventana crece y el resto se oscurece",
        "d6": "Clic en otra ventana: vuelve a su lugar",
        "dStatic": "Cada app se abre en su zona",
        "dynamic": "dinámica",
        "demo": "Cómo funciona Venecita Dock",
        "d7": "Abrís la notebook: se usa la distribución con dos pantallas",
        "d8": "La cerrás: todo vuelve al monitor, solo",
        "d9": "En la oficina, dos monitores: pasa solo al perfil Trabajo",
        "profileLbl": "Perfil",
        "home": "Casa",
        "work": "Trabajo",
    },
}


def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def svg(w, h, body, title, style=""):
    style = f"<style>{style}</style>\n" if style else ""
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" role="img" aria-label="{esc(title)}">
<title>{esc(title)}</title>
<defs>
  <linearGradient id="card" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#f8fafc"/><stop offset="1" stop-color="#eef2f7"/>
  </linearGradient>
  <linearGradient id="wall" x1="0" y1="0" x2="1" y2="1">
    <stop offset="0" stop-color="#1e3a5f"/><stop offset="1" stop-color="#3b2f63"/>
  </linearGradient>
  <filter id="shadow" x="-10%" y="-10%" width="120%" height="130%">
    <feDropShadow dx="0" dy="2" stdDeviation="2.5" flood-color="#0f172a" flood-opacity="0.25"/>
  </filter>
  <marker id="arrow" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M0,0 L10,5 L0,10 z" fill="#64748b"/>
  </marker>
</defs>
{style}<rect x="0.5" y="0.5" width="{w - 1}" height="{h - 1}" rx="18" fill="url(#card)" stroke="#d6dde8"/>
<g font-family="{FONT}">
{body}
</g>
</svg>
"""


def text(x, y, s, size=14, weight=500, fill="#1e293b", anchor="middle", family=None, opacity=1):
    fam = f' font-family="{family}"' if family else ""
    op = f' opacity="{opacity}"' if opacity != 1 else ""
    return (f'<text x="{x}" y="{y}" font-size="{size}" font-weight="{weight}" fill="{fill}" '
            f'text-anchor="{anchor}"{fam}{op}>{esc(s)}</text>')


def badge(x, y, n):
    return (f'<circle cx="{x}" cy="{y}" r="13" fill="#2563eb"/>'
            + text(x, y + 5, str(n), 14, 700, "#ffffff"))


def screen_rect(x, y, w, h):
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="3" fill="url(#wall)"/>'


def monitor(x, y, w, h):
    """Monitor with bezel and stand; returns (svg, screen rect)."""
    b = 7
    out = [
        f'<rect x="{x + w / 2 - 14}" y="{y + h}" width="28" height="16" fill="#94a3b8"/>',
        f'<rect x="{x + w / 2 - 46}" y="{y + h + 14}" width="92" height="6" rx="3" fill="#94a3b8"/>',
        f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="9" fill="#0f172a" filter="url(#shadow)"/>',
        screen_rect(x + b, y + b, w - 2 * b, h - 2 * b),
    ]
    return "".join(out), (x + b, y + b, w - 2 * b, h - 2 * b)


def laptop(x, y, w, h, closed=False):
    if closed:
        # Seen from the front: a thin closed lid on the base.
        return (f'<rect x="{x - 10}" y="{y + h - 8}" width="{w + 20}" height="10" rx="4" fill="#94a3b8"/>'
                f'<rect x="{x - 4}" y="{y + h - 14}" width="{w + 8}" height="7" rx="3" fill="#cbd5e1"/>'), None
    b = 6
    out = [
        f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="8" fill="#0f172a" filter="url(#shadow)"/>',
        screen_rect(x + b, y + b, w - 2 * b, h - 2 * b),
        f'<path d="M{x - 16},{y + h + 10} L{x + w + 16},{y + h + 10} L{x + w + 4},{y + h} L{x - 4},{y + h} z" fill="#94a3b8"/>',
    ]
    return "".join(out), (x + b, y + b, w - 2 * b, h - 2 * b)


def grid(scr, cols=8, rows=4, opacity=0.22, dashed=True):
    x, y, w, h = scr
    dash = ' stroke-dasharray="3 3"' if dashed else ""
    out = []
    for c in range(1, cols):
        gx = x + w * c / cols
        out.append(f'<line x1="{gx:.1f}" y1="{y}" x2="{gx:.1f}" y2="{y + h}" stroke="#ffffff" stroke-opacity="{opacity}"{dash}/>')
    for r in range(1, rows):
        gy = y + h * r / rows
        out.append(f'<line x1="{x}" y1="{gy:.1f}" x2="{x + w}" y2="{gy:.1f}" stroke="#ffffff" stroke-opacity="{opacity}"{dash}/>')
    return "".join(out)


def cell_rect(scr, col, row, cw, ch, cols=8, rows=4, gap=3):
    x, y, w, h = scr
    return (x + w * col / cols + gap, y + h * row / rows + gap,
            w * cw / cols - 2 * gap, h * ch / rows - 2 * gap)


def window(r, app, title=None, dim=False, highlight=False):
    x, y, w, h = r
    color = APP.get(app, "#64748b")
    bar = min(16, max(11, h * 0.14))
    fs = max(8, min(12, bar - 4))
    out = ['<g opacity="0.35">' if dim else "<g>"]
    out.append(f'<rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" rx="5" fill="#f8fafc" filter="url(#shadow)"/>')
    out.append(f'<path d="M{x:.1f},{y + bar:.1f} v{-(bar - 5):.1f} a5,5 0 0 1 5,-5 h{w - 10:.1f} a5,5 0 0 1 5,5 v{bar - 5:.1f} z" fill="{color}"/>')
    dots = w > 110
    if dots:
        for i, c in enumerate(["#ff5f57", "#febc2e", "#28c840"]):
            out.append(f'<circle cx="{x + 8 + i * 8:.1f}" cy="{y + bar / 2:.1f}" r="2.4" fill="{c}"/>')
    label = title or app
    if w > 40:
        tx = x + 34 if dots else x + 7
        out.append(text(round(tx, 1), round(y + bar / 2 + fs / 2.8, 1), label, fs, 600, "#ffffff", "start"))
    # Placeholder content lines.
    ly = y + bar + 9
    i = 0
    while ly < y + h - 6 and i < 6:
        lw = (w - 16) * (0.85 if i % 3 == 0 else 0.6 if i % 3 == 1 else 0.72)
        out.append(f'<rect x="{x + 8:.1f}" y="{ly:.1f}" width="{lw:.1f}" height="4" rx="2" fill="{color}" opacity="0.18"/>')
        ly += 10
        i += 1
    if highlight:
        out.append(f'<rect x="{x - 2:.1f}" y="{y - 2:.1f}" width="{w + 4:.1f}" height="{h + 4:.1f}" rx="7" fill="none" stroke="#facc15" stroke-width="2.5"/>')
    out.append("</g>")
    return "".join(out)


def arrow(x1, y1, x2, y2, curve=0):
    if curve:
        mx, my = (x1 + x2) / 2, (y1 + y2) / 2 - curve
        return f'<path d="M{x1},{y1} Q{mx},{my} {x2},{y2}" fill="none" stroke="#64748b" stroke-width="2" marker-end="url(#arrow)"/>'
    return f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="#64748b" stroke-width="2" marker-end="url(#arrow)"/>'


def keycap(x, y, label):
    w = 9 * len(label) + 16
    return (f'<rect x="{x}" y="{y}" width="{w}" height="26" rx="6" fill="#ffffff" stroke="#cbd5e1"/>'
            f'<rect x="{x}" y="{y + 22}" width="{w}" height="4" rx="2" fill="#cbd5e1"/>'
            + text(x + w / 2, y + 17, label, 13, 600, "#334155", family=MONO)), w


# ---------------------------------------------------------------------------------------

def how_it_works(t):
    W, H = 960, 340
    out = []
    xs = [40, 350, 660]
    titles = [t["s1"], t["s2"], t["s3"]]
    for i, x in enumerate(xs):
        out.append(badge(x + 14, 38, i + 1))
        out.append(text(x + 34, 43, titles[i], 15, 650, "#0f172a", "start"))
    # 1: the grid.
    m, scr = monitor(xs[0] + 10, 80, 240, 150)
    out.append(m)
    out.append(grid(scr, opacity=0.5))
    out.append(text(xs[0] + 130, 300, t["cells"], 13, 500, "#475569"))
    # 2: a selection on the grid and the rule.
    m, scr = monitor(xs[1] + 10, 80, 240, 150)
    out.append(m)
    out.append(grid(scr, opacity=0.35))
    sx, sy, sw, sh = cell_rect(scr, 0, 0, 5, 4, gap=2)
    out.append(f'<rect x="{sx:.1f}" y="{sy:.1f}" width="{sw:.1f}" height="{sh:.1f}" rx="4" fill="{APP["Code"]}" fill-opacity="0.45" stroke="#93c5fd" stroke-width="2"/>')
    out.append(text(sx + sw / 2, sy + sh / 2 + 5, "Code", 15, 700, "#ffffff"))
    sx, sy, sw, sh = cell_rect(scr, 5, 0, 3, 2, gap=2)
    out.append(f'<rect x="{sx:.1f}" y="{sy:.1f}" width="{sw:.1f}" height="{sh:.1f}" rx="4" fill="{APP["Chrome"]}" fill-opacity="0.45" stroke="#fdba74" stroke-width="2"/>')
    out.append(text(sx + sw / 2, sy + sh / 2 + 4, "Chrome", 12, 700, "#ffffff"))
    code = '"code" → col 0, row 0, 5 × 4'
    out.append(f'<rect x="{xs[1] + 4}" y="282" width="252" height="30" rx="7" fill="#0f172a"/>')
    out.append(text(xs[1] + 130, 302, code, 11, 500, "#e2e8f0", family=MONO))
    # 3: placed windows.
    m, scr = monitor(xs[2] + 10, 80, 240, 150)
    out.append(m)
    out.append(window(cell_rect(scr, 0, 0, 5, 4), "Code"))
    out.append(window(cell_rect(scr, 5, 0, 3, 2), "Chrome"))
    out.append(window(cell_rect(scr, 5, 2, 3, 2), "Slack"))
    # Arrows between steps.
    out.append(arrow(292, 155, 340, 155))
    out.append(arrow(602, 155, 650, 155))
    # App launch hint on step 3.
    out.append(f'<rect x="{xs[2] + 50}" y="282" width="160" height="30" rx="15" fill="#e0e7ff"/>')
    out.append(text(xs[2] + 130, 302, "Code ✓  Chrome ✓  Slack ✓", 12, 600, "#3730a3"))
    return svg(W, H, "\n".join(out), t["s3"])


def profiles(t):
    W, H = 960, 400
    out = []
    out.append(text(W / 2, 40, t["profile"], 17, 700, "#0f172a"))
    # Left: laptop open + monitor.
    out.append(text(235, 76, t["open"], 15, 650, "#2563eb"))
    m, big = monitor(60, 96, 260, 160)
    out.append(m)
    out.append(window(cell_rect(big, 0, 0, 4, 4), "Code"))
    out.append(window(cell_rect(big, 4, 0, 4, 4), "Firefox"))
    lp, small = laptop(340, 166, 130, 86)
    out.append(lp)
    out.append(window(cell_rect(small, 0, 0, 8, 4, gap=2), "Mail"))
    # Right: laptop closed, only the monitor.
    out.append(text(725, 76, t["closed"], 15, 650, "#2563eb"))
    m, big2 = monitor(560, 96, 260, 160)
    out.append(m)
    out.append(window(cell_rect(big2, 0, 0, 4, 4), "Code"))
    out.append(window(cell_rect(big2, 4, 0, 4, 2), "Firefox"))
    out.append(window(cell_rect(big2, 4, 2, 4, 2), "Mail"))
    lp, _ = laptop(840, 166, 90, 86, closed=True)
    out.append(lp)
    # Middle explanation.
    out.append(arrow(492, 170, 548, 170))
    out.append(text(W / 2, 330, t["detect"], 15, 600, "#0f172a"))
    out.append(text(W / 2, 352, t["same"], 14, 500, "#475569"))
    return svg(W, H, "\n".join(out), t["profile"])


def tab_bar(x, y, w, tabs, active):
    out = [f'<rect x="{x}" y="{y}" width="{w}" height="24" rx="7" fill="#1f2937" fill-opacity="0.92" filter="url(#shadow)"/>']
    tw = (w - 6) / len(tabs)
    for i, (app, label) in enumerate(tabs):
        tx = x + 3 + i * tw
        if i == active:
            out.append(f'<rect x="{tx:.1f}" y="{y + 3}" width="{tw - 2:.1f}" height="18" rx="5" fill="#475569"/>')
        out.append(f'<circle cx="{tx + 11:.1f}" cy="{y + 12}" r="4" fill="{APP[app]}"/>')
        out.append(text(round(tx + 20, 1), y + 16, label, 11, 600 if i == active else 500,
                        "#ffffff" if i == active else "#cbd5e1", "start"))
    return "".join(out)


def tabs(t):
    W, H = 960, 400
    out = []
    # Left: a stack of three Chrome windows.
    out.append(text(240, 44, t["stack"], 15, 650, "#0f172a"))
    m, scr = monitor(60, 70, 360, 220)
    out.append(m)
    out.append(grid(scr, opacity=0.15))
    out.append(window(cell_rect(scr, 0, 0, 3, 4), "Code"))
    zx, zy, zw, zh = cell_rect(scr, 3, 0, 5, 4)
    out.append(window((zx, zy + 30, zw, zh - 30), "Chrome", "Chrome — Docs"))
    out.append(tab_bar(zx, zy, zw, [("Chrome", "Docs"), ("Chrome", "Mail"), ("Chrome", "GitHub")], 0))
    k1, w1 = keycap(118, 334, "⌃⌥]")
    k2, w2 = keycap(118 + w1 + 10, 334, "⌃⌥[")
    out.append(k1 + k2)
    out.append(text(118 + w1 + w2 + 22, 352, t["switch"], 13, 500, "#475569", "start"))
    # Right: the dynamic zone.
    out.append(text(720, 44, t["dyn"], 15, 650, "#0f172a"))
    m, scr = monitor(540, 70, 360, 220)
    out.append(m)
    out.append(grid(scr, opacity=0.15))
    out.append(window(cell_rect(scr, 0, 0, 3, 4), "Code"))
    dx, dy, dw, dh = cell_rect(scr, 3, 0, 5, 4)
    out.append(f'<rect x="{dx - 1:.1f}" y="{dy - 1:.1f}" width="{dw + 2:.1f}" height="{dh + 2:.1f}" rx="6" fill="none" stroke="#facc15" stroke-width="2" stroke-dasharray="5 4"/>')
    out.append(window((dx, dy + 30, dw, dh - 30), "Spotify"))
    out.append(tab_bar(dx, dy, dw, [("Spotify", "Spotify"), ("Notes", "Notes"), ("Figma", "Figma")], 0))
    # Apps without a rule flying in.
    for i, app in enumerate(["Notes", "Figma"]):
        bx = 700 + i * 90
        out.append(f'<rect x="{bx}" y="326" width="74" height="28" rx="14" fill="#ffffff" stroke="#cbd5e1"/>')
        out.append(f'<circle cx="{bx + 14}" cy="340" r="5" fill="{APP[app]}"/>')
        out.append(text(bx + 44, 345, app, 12, 600, "#334155"))
    out.append(text(632, 345, t["noRule"] + " →", 12, 600, "#64748b"))
    out.append(arrow(780, 322, dx + dw / 2, dy + dh + 6, curve=0))
    return svg(W, H, "\n".join(out), t["stack"] + " / " + t["dyn"])


def focus(t):
    W, H = 960, 400
    out = []
    out.append(text(240, 44, t["before"], 15, 650, "#0f172a"))
    m, scr = monitor(60, 70, 360, 220)
    out.append(m)
    out.append(window(cell_rect(scr, 0, 0, 4, 4), "Code"))
    out.append(window(cell_rect(scr, 4, 0, 4, 2), "Chrome"))
    out.append(window(cell_rect(scr, 4, 2, 2, 2), "Slack"))
    out.append(window(cell_rect(scr, 6, 2, 2, 2), "Terminal"))
    out.append(text(720, 44, t["after"], 15, 650, "#0f172a"))
    m, scr = monitor(540, 70, 360, 220)
    out.append(m)
    out.append(window(cell_rect(scr, 0, 0, 4, 4), "Code", dim=True))
    out.append(window(cell_rect(scr, 4, 2, 2, 2), "Slack", dim=True))
    out.append(window(cell_rect(scr, 6, 2, 2, 2), "Terminal", dim=True))
    x, y, w, h = scr
    out.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="3" fill="#000000" fill-opacity="0.45"/>')
    fw, fh = w * 0.8, h * 0.8
    out.append(window((x + (w - fw) / 2, y + (h - fh) / 2, fw, fh), "Chrome", highlight=True))
    out.append(arrow(436, 180, 524, 180))
    k, kw = keycap(450, 140, "⌃⌥F")
    out.append(k)
    out.append(text(W / 2, 345, t["focusNote"], 14, 500, "#475569"))
    return svg(W, H, "\n".join(out), t["after"])


def api(t):
    W, H = 960, 260
    out = []
    # Stream deck.
    out.append(f'<rect x="60" y="60" width="200" height="130" rx="14" fill="#0f172a" filter="url(#shadow)"/>')
    colors = ["#2f80ed", "#e8833a", "#1fa463", "#8e44ad", "#e5484d", "#d4a017"]
    icons = ["⌂", "▦", "◐", "⇄", "⏸", "★"]
    for i in range(6):
        bx = 78 + (i % 3) * 58
        by = 78 + (i // 3) * 52
        out.append(f'<rect x="{bx}" y="{by}" width="48" height="42" rx="8" fill="{colors[i]}"/>')
        out.append(text(bx + 24, by + 28, icons[i], 18, 700, "#ffffff"))
    out.append(text(160, 220, t["deck"], 13, 600, "#334155"))
    # Arrow with the transport.
    out.append(arrow(280, 125, 660, 125))
    out.append(text(470, 112, t["token"], 13, 600, "#334155"))
    out.append(f'<rect x="350" y="140" width="240" height="30" rx="7" fill="#0f172a"/>')
    out.append(text(470, 160, "POST /v1/actions/relayout-all", 11.5, 500, "#e2e8f0", family=MONO))
    out.append(text(470, 192, t["cli"] + ":", 12, 500, "#64748b"))
    out.append(text(470, 212, "venecita profile activate Casa", 12, 600, "#334155", family=MONO))
    # The app.
    out.append(f'<image href="{ICON}" x="700" y="70" width="110" height="110"/>')
    out.append(text(755, 210, t["app"], 14, 700, "#0f172a"))
    return svg(W, H, "\n".join(out), t["token"])


# --- Animated demo ---------------------------------------------------------------------
# One CSS loop (no script, so GitHub shows it inside <img>). Every element is drawn in its
# final place and the keyframes move it there; with reduced motion the animations are off
# and the drawing is that final, static state.

LOOP = 35.0  # seconds
EASE_OUT = "cubic-bezier(0.23, 1, 0.32, 1)"
EASE_IN_OUT = "cubic-bezier(0.77, 0, 0.175, 1)"


def fit(src, dst):
    """CSS transform that maps an element drawn at `dst` onto rectangle `src`."""
    sx, sy, sw, sh = src
    dx, dy, dw, dh = dst
    return (f"translate({sx:.1f}px,{sy:.1f}px) scale({sw / dw:.3f},{sh / dh:.3f}) "
            f"translate({-dx:.1f}px,{-dy:.1f}px)")


def keyframes(name, stops):
    """`stops`: (seconds, {property: value}, easing of the segment that follows)."""
    out = [f"@keyframes {name}{{"]
    for t, props, ease in stops:
        body = ";".join(f"{k}:{v}" for k, v in props.items())
        if ease:
            body += f";animation-timing-function:{ease}"
        out.append(f"{t / LOOP * 100:.3f}%{{{body}}}")
    out.append("}")
    return "".join(out)


def anim(cls, stops):
    return (keyframes(cls, stops)
            + f".{cls}{{animation:{cls} {LOOP}s infinite both;"
            "transform-box:view-box;transform-origin:0 0}")


def shown(a, b, fade=0.3, out=0.25, slide=6):
    """Visible from `a` to `b`: fades (and slides) in, fades out."""
    hidden = {"opacity": 0, "transform": f"translateY({slide}px)"}
    still = {"opacity": 1, "transform": "translateY(0px)"}
    return [(0, hidden, None), (a, hidden, EASE_OUT), (a + fade, still, None),
            (b, still, "ease"), (b + out, {"opacity": 0, "transform": "translateY(0px)"}, None),
            (LOOP, {"opacity": 0, "transform": "translateY(0px)"}, None)]


def track(cls, points, base=""):
    """An element's animation from (seconds, opacity, transform, easing after) points."""
    stops = [(t, {"opacity": op, "transform": tr}, ease) for t, op, tr, ease in points]
    return anim(cls, stops) + (f".{cls}{{{base}}}" if base else "")


def demo(t):
    W, H = 960, 440
    css, out = [], []
    N = "none"
    IO, OUT = EASE_IN_OUT, EASE_OUT
    # Two desks: Home (one monitor and the laptop) for most of the loop, and Work (two
    # monitors side by side, laptop closed) at the end. Home's parts are all `deskH`.
    m, scr = monitor(70, 80, 560, 300)
    out.append(f'<g class="deskH">{m}</g>')
    x, y, w, h = scr
    css.append(track("deskH", [(0, 1, N, None), (29.6, 1, N, "ease"), (29.9, 0, N, None),
                               (34.5, 0, N, OUT), (34.9, 1, N, None), (LOOP, 1, N, None)]))
    mw, mh, gap = 392, 226, 16
    m1, scr_l = monitor(W / 2 - mw - gap / 2, 78, mw, mh)
    m2, scr_r = monitor(W / 2 + gap / 2, 78, mw, mh)
    out.append(f'<g class="deskW">{m1}{m2}{grid(scr_l, opacity=0.16)}{grid(scr_r, opacity=0.16)}</g>')
    low = "translateY(8px)"
    css.append(track("deskW", [(0, 0, low, None), (29.9, 0, low, OUT), (30.25, 1, N, None),
                               (34.2, 1, N, "ease"), (34.5, 0, N, None), (LOOP, 0, N, None)],
                     "opacity:0"))

    # Captions, one per step; the static one only shows with reduced motion.
    steps = [("d1", 0.2, 3.8), ("d2", 4.0, 7.8), ("d3", 8.0, 12.2), ("d4", 12.5, 16.2),
             ("d5", 16.5, 19.4), ("d6", 19.6, 21.2), ("d7", 21.4, 25.4), ("d8", 25.6, 29.3),
             ("d9", 29.5, 34.3)]
    for i, (key, a, b) in enumerate(steps):
        out.append(f'<g class="c{i}">' + text(W / 2, 48, t[key], 17, 650, "#0f172a") + "</g>")
        css.append(anim(f"c{i}", shown(a, b)))
    out.append('<g class="cs">' + text(W / 2, 48, t["dStatic"], 17, 650, "#0f172a") + "</g>")
    css.append(anim("cs", [(0, {"opacity": 0}, None), (LOOP, {"opacity": 0}, None)]))

    # The laptop: its base always, the lid opening and closing (scaled from the hinge).
    lx, ly, lw, lh = 680, 236, 210, 134
    lp, lscr = laptop(lx, ly, lw, lh)
    base = lp[lp.index("<path"):]
    lid = lp[:lp.index("<path")]
    out.append(f'<g class="deskH">{base}<g class="lid">{lid}{grid(lscr, opacity=0.3)}</g></g>')
    lid_rect = (lx, ly, lw, lh)
    closed = fit((lx, ly + lh - 7, lw, 7), lid_rect)
    css.append(track("lid", [(0, 1, closed, None), (21.6, 1, closed, IO), (22.2, 1, N, None),
                             (26.0, 1, N, IO), (26.5, 1, closed, None), (LOOP, 1, closed, None)],
                     f"transform:{closed}"))

    # Profile and layout in use: above the laptop at Home, below the two monitors at Work.
    def card(px, py, profile, layouts):
        s = (f'<rect x="{px}" y="{py}" width="{lw + 10}" height="58" rx="12" fill="#ffffff" stroke="#cbd5e1"/>'
             + text(px + 14, py + 24, t["profileLbl"], 12, 600, "#64748b", "start")
             + text(px + 66, py + 24, profile, 15, 700, "#0f172a", "start"))
        for cls, label in layouts:
            label = text(px + 14, py + 46, label, 13, 600, "#2563eb", "start")
            s += f'<g class="{cls}">{label}</g>' if cls else label
        return s

    out.append('<g class="deskH">' + card(lx - 5, 104, t["home"],
                                          [("lClosed", t["closed"]), ("lOpen", t["open"])]) + "</g>")
    out.append('<g class="cardW">' + card(W / 2 - lw / 2 - 5, 346, t["work"], [(None, t["closed"])]) + "</g>")
    css.append(anim("cardW", shown(30.6, 34.2)) + ".cardW{opacity:0}")
    css.append(track("lClosed", [(0, 1, N, None), (21.7, 1, N, "ease"), (21.9, 0, N, None),
                                 (26.5, 0, N, OUT), (26.8, 1, N, None), (LOOP, 1, N, None)]))
    css.append(track("lOpen", [(0, 0, N, None), (21.9, 0, N, OUT), (22.2, 1, N, None),
                               (26.3, 1, N, "ease"), (26.5, 0, N, None), (LOOP, 0, N, None)], "opacity:0"))

    # 1. The grid.
    out.append('<g class="deskH"><g class="grid">' + grid(scr, opacity=0.45) + "</g></g>")
    css.append(anim("grid", [(0, {"opacity": 0.25}, OUT), (0.8, {"opacity": 1}, None),
                             (8.0, {"opacity": 1}, "ease"), (8.6, {"opacity": 0.35}, None),
                             (34.4, {"opacity": 0.35}, "ease"), (LOOP, {"opacity": 0.25}, None)]))

    # Zones of each layout.
    code_z = cell_rect(scr, 0, 0, 5, 4)
    chrome_z = cell_rect(scr, 5, 0, 3, 2)
    dyn_z = cell_rect(scr, 5, 2, 3, 2)
    chrome_open = cell_rect(scr, 5, 0, 3, 4)          # Home, laptop open
    laptop_dyn = cell_rect(lscr, 0, 0, 8, 4, gap=2)    # its dynamic zone: the laptop
    code_w = cell_rect(scr_l, 0, 0, 8, 4)              # Work: the left monitor,
    chrome_w = cell_rect(scr_r, 0, 0, 5, 4)            # Chrome and the dynamic zone
    dyn_w = cell_rect(scr_r, 5, 0, 3, 4)               # on the right one

    def below(zone):
        zx, zy, zw, zh = zone
        return (zx, zy + 28, zw, zh - 28)

    # 2. The zones, one after the other.
    zones = [(code_z, "Code", APP["Code"], "#93c5fd", ""),
             (chrome_z, "Chrome", APP["Chrome"], "#fdba74", ""),
             (dyn_z, t["dynamic"], "#facc15", "#facc15", ' stroke-dasharray="6 4"')]
    for i, ((zx, zy, zw, zh), label, fill, stroke, dash) in enumerate(zones):
        out.append(f'<g class="z{i}"><rect x="{zx:.1f}" y="{zy:.1f}" width="{zw:.1f}" height="{zh:.1f}" rx="5" '
                   f'fill="{fill}" fill-opacity="0.4" stroke="{stroke}" stroke-width="2"{dash}/>'
                   + text(zx + zw / 2, zy + zh / 2 + 6, label, 16, 700, "#ffffff") + "</g>")
        a = 4.3 + i * 0.6
        css.append(track(f"z{i}", [(0, 0, N, None), (a, 0, N, OUT), (a + 0.35, 1, N, None),
                                   (8.0, 1, N, "ease"), (8.4, 0, N, None), (LOOP, 0, N, None)],
                         "opacity:0"))

    # 3. Windows open somewhere, go to their zone, and follow every layout change. A window
    # is one copy per place it rests in, drawn at that size (text isn't stretched): each copy
    # slides in from the previous place, then hands over to the next one.
    def mess(dx, dy, mw, mh):
        return (x + dx, y + dy, mw, mh)

    def copies(cls, app, title, appear, states, rest, focus_hide=False, hidden_end=False):
        """`states`: (start, rect) after opening at `states[0]`; `rest`: the copy shown
        with reduced motion (Home, laptop closed)."""
        for k, (start, rect) in enumerate(states):
            end = states[k + 1][0] if k + 1 < len(states) else None
            name = f"{cls}{k}"
            out.append(f'<g class="{name}">' + window(rect, app, title) + "</g>")
            if k == 0:
                pts = [(0, 0, N, None), (appear, 0, N, OUT), (appear + 0.3, 1, N, None)]
            else:
                came = fit(states[k - 1][1], rect)
                pts = [(0, 0, came, None), (start - 0.01, 0, came, None),
                       (start, 1, came, IO), (start + 0.7, 1, N, None)]
            if focus_hide and start < 17.0 and (end is None or end > 17.0):
                pts += [(17.0, 1, N, None), (17.05, 0, N, None), (20.85, 0, N, None),
                        (20.9, 1, N, None)]
            if end is None:
                # A window under another one leaves while still covered, so it doesn't
                # show through the one on top as that fades out.
                gone = (34.0, 34.05) if hidden_end else (34.2, 34.6)
                pts += [(gone[0], 1, N, "ease"), (gone[1], 0, N, None), (LOOP, 0, N, None)]
            else:
                pts += [(end - 0.01, 1, N, None), (end, 0, N, None), (LOOP, 0, N, None)]
            css.append(track(name, pts, "" if k == rest else "opacity:0"))

    bar = below(chrome_z)
    copies("wcode", "Code", None, 8.4,
           [(8.4, mess(150, 70, 230, 150)), (9.0, code_z), (30.2, code_w)], 1, True)
    copies("wchrome", "Chrome", "Chrome — Docs", 9.0,
           [(9.0, mess(260, 120, 200, 130)), (9.6, chrome_z), (13.6, bar),
            (22.4, below(chrome_open)), (26.0, bar), (30.2, below(chrome_w))], 2,
           hidden_end=True)
    copies("wslack", "Slack", None, 9.6,
           [(9.6, mess(60, 150, 180, 110)), (10.2, dyn_z), (22.4, laptop_dyn), (25.8, dyn_z),
            (30.2, dyn_w)], 1)
    copies("wchrome2", "Chrome", "Chrome — Mail", 12.8,
           [(12.8, mess(190, 40, 200, 130)), (13.4, bar), (22.4, below(chrome_open)),
            (26.0, bar), (30.2, below(chrome_w))], 1)

    # 4. Tab bars: Home's Chrome zone, then Work's (hidden in focus mode).
    zx, zy, zw, _ = chrome_z
    out.append('<g class="tabs">' + tab_bar(zx, zy, zw, [("Chrome", "Docs"), ("Chrome", "Mail")], 1) + "</g>")
    css.append(track("tabs", [(0, 0, N, None), (14.0, 0, N, OUT), (14.3, 1, N, None),
                              (16.95, 1, N, "ease"), (17.2, 0, N, None), (20.9, 0, N, OUT),
                              (21.1, 1, N, None), (29.9, 1, N, "ease"), (30.1, 0, N, None),
                              (LOOP, 0, N, None)]))
    zx, zy, zw, _ = chrome_w
    out.append('<g class="tabsW">' + tab_bar(zx, zy, zw, [("Chrome", "Docs"), ("Chrome", "Mail")], 1) + "</g>")
    css.append(track("tabsW", [(0, 0, N, None), (30.85, 0, N, OUT), (31.15, 1, N, None),
                               (34.2, 1, N, "ease"), (34.6, 0, N, None), (LOOP, 0, N, None)],
                     "opacity:0"))

    # 5. Focus mode: dim layer, then Code grows to 80 % of the screen.
    out.append(f'<rect class="dim" x="{x}" y="{y}" width="{w}" height="{h}" rx="3" fill="#000000"/>')
    css.append(anim("dim", [(0, {"opacity": 0}, None), (17.0, {"opacity": 0}, OUT),
                            (17.5, {"opacity": 0.5}, None), (20.2, {"opacity": 0.5}, OUT),
                            (20.7, {"opacity": 0}, None), (LOOP, {"opacity": 0}, None)]))
    css.append(".dim{opacity:0}")
    big = (x + w * 0.1, y + h * 0.1, w * 0.8, h * 0.8)
    out.append('<g class="big">' + window(big, "Code", highlight=True) + "</g>")
    small = fit(code_z, big)
    css.append(track("big", [(0, 0, small, None), (17.0, 0, small, None), (17.05, 1, small, IO),
                             (17.7, 1, N, None), (20.2, 1, N, IO), (20.85, 1, small, None),
                             (20.9, 0, small, None), (LOOP, 0, small, None)], "opacity:0"))

    # The shortcut, next to the monitor.
    k, _ = keycap(lx + 70, 184, "⌃⌥F")
    out.append(f'<g class="keyF">{k}</g>')
    css.append(anim("keyF", shown(16.6, 19.6, slide=8)) + ".keyF{opacity:0}")

    # A click on Slack, where the enlarged window leaves it uncovered, ends focus mode.
    sx, sy, sw, sh = dyn_z
    cx, cy = x + w * 0.945, sy + sh * 0.55
    cursor = (f'<path d="M{cx:.1f},{cy:.1f} l0,18 l5,-4.5 l3.5,7.5 l3,-1.4 l-3.4,-7.4 l6.6,-0.6 z" '
              'fill="#ffffff" stroke="#0f172a" stroke-width="1.4" stroke-linejoin="round"/>')
    out.append(f'<g class="cur">{cursor}</g>')
    away, here = "translate(40px,50px)", "translate(0px,0px)"
    css.append(track("cur", [(0, 0, away, None), (19.2, 0, away, OUT), (19.8, 1, here, None),
                             (20.05, 1, here, None), (20.15, 1, "translate(1px,1.5px)", None),
                             (20.25, 1, here, None), (20.8, 1, here, "ease"), (21.2, 0, here, None),
                             (LOOP, 0, away, None)], "opacity:0"))
    out.append(f'<circle class="ripple" cx="{cx:.1f}" cy="{cy:.1f}" r="14" fill="none" stroke="#facc15" stroke-width="2.5"/>')
    css.append(keyframes("ripple", [(0, {"opacity": 0, "transform": "scale(0.4)"}, None),
                                    (20.1, {"opacity": 0, "transform": "scale(0.4)"}, OUT),
                                    (20.15, {"opacity": 1, "transform": "scale(0.4)"}, OUT),
                                    (20.6, {"opacity": 0, "transform": "scale(1.4)"}, None),
                                    (LOOP, {"opacity": 0, "transform": "scale(1.4)"}, None)])
               + f".ripple{{opacity:0;animation:ripple {LOOP}s infinite both;"
               "transform-box:fill-box;transform-origin:center}")

    # Reduced motion: no animation; the drawing is the resting state (Home, laptop closed,
    # windows in their zones with tabs, the static caption).
    captions = ",".join(f".c{i}" for i in range(len(steps)))
    css.append("@media (prefers-reduced-motion: reduce){*{animation:none!important}"
               f".cs{{opacity:1}}{captions}{{opacity:0}}}}")
    return svg(W, H, "\n".join(out), t["demo"], "".join(css))

ICON_FILE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "docs", "images", "app-icon.png")
ICON = "data:image/png;base64," + base64.b64encode(open(ICON_FILE, "rb").read()).decode()


def main(outdir):
    os.makedirs(outdir, exist_ok=True)
    for lang, t in T.items():
        for name, fn in [("how-it-works", how_it_works), ("profiles", profiles),
                         ("tabs", tabs), ("focus", focus), ("api", api), ("demo", demo)]:
            with open(os.path.join(outdir, f"{name}-{lang}.svg"), "w", encoding="utf-8") as f:
                f.write(fn(t))


if __name__ == "__main__":
    main(sys.argv[1])
