"""Draw docs/assets/fineweb-speed.svg from tools/recipe_bench.py results.

Usage: make_chart.py <results.json> <out.svg> [wall|cpu] [light|dark]
  `dark` bakes the dark colors in as the base, for previewing with renderers that ignore @media.

Bar lengths and labels both come from the medians in the results file, on one shared seconds axis. Emphasis form:
datatrove-rs in the accent blue, datatrove in a neutral gray. Colors were checked with the dataviz skill's
validate_palette.js against both surfaces (light #fcfcfb, dark #1a1a19): every check passes except the chroma
floor on the gray, which is the point of a neutral.
"""

import json
import sys
from html import escape

W = 720
FONT = "system-ui,-apple-system,Segoe UI,Helvetica,Arial,sans-serif"
MONO = "ui-monospace,SFMono-Regular,Menlo,Consolas,monospace"
MONO_W = 0.6 * 13  # advance of one 13px monospace glyph
BAR, PAIR_GAP, GROUP_GAP = 20, 4, 22
FONTS = {"t": f"600 15px {FONT}", "s": f"13px {FONT}", "g": f"600 13px {FONT}", "k": f"13px {FONT}",
         "v": f"13px {MONO}", "vo": f"600 13px {MONO}", "m": f"12px {FONT}"}
# class -> (light, dark): text tokens primary/secondary/muted, series accent and neutral, baseline
COLORS = {"t": ("#0b0b0b", "#f0efec"), "g": ("#0b0b0b", "#f0efec"), "vo": ("#0b0b0b", "#f0efec"),
          "s": ("#52514e", "#c3c2b7"), "k": ("#52514e", "#c3c2b7"), "v": ("#52514e", "#c3c2b7"),
          "m": ("#898781", "#898781"), "ours": ("#2a78d6", "#3987e5"), "other": ("#898781", "#6e6d68"),
          "axis": ("#c3c2b7", "#383835")}


def style(base="light"):
    """CSS with `base` colors, plus a prefers-color-scheme dark override when the base is light."""
    def rules(i):
        return "\n".join(f".{c}{{{'stroke' if c == 'axis' else 'fill'}:{pair[i]}}}" for c, pair in COLORS.items())

    fonts = "\n".join(f".{c}{{font:{f}}}" for c, f in FONTS.items())
    dark = "" if base == "dark" else f"\n@media (prefers-color-scheme:dark){{\n{rules(1)}\n}}"
    return f"<style>\n{fonts}\n{rules(base == 'dark')}{dark}\n</style>"

SCENARIOS = {
    "fineweb": "FineWeb recipe, 5,000 Common Crawl pages (WARC to filtered JSONL)",
    "fineweb2": "FineWeb-2 German, 4,000 documents (quality filters and MinHash signatures)",
}


def bar(cls, x, y, w):
    """Horizontal bar: square at the baseline, 4px rounded data end."""
    r = min(4, w / 2)
    return (f'<path class="{cls}" d="M{x},{y} h{w - r:.2f} a{r},{r} 0 0 1 {r},{r} v{BAR - 2 * r} '
            f'a{r},{r} 0 0 1 {-r},{r} h{-(w - r):.2f} z"/>')


def text(cls, x, y, s, anchor=None):
    a = f' text-anchor="{anchor}"' if anchor else ""
    return f'<text class="{cls}" x="{x:.2f}" y="{y}"{a}>{escape(s)}</text>'


def labels(med):
    """(datatrove label, datatrove-rs label) for one scenario's medians."""
    def secs(v):
        return f"{v:.2f} s" if v < 10 else f"{v:.1f} s"

    return secs(med["pure"]), f"{secs(med['rs'])}  ({med['pure'] / med['rs']:.1f}x faster)"


def scale(groups, x0):
    """Pixels per second such that every bar plus its tip label fits in W."""
    k = float("inf")
    for med in groups.values():
        for v, lab in zip((med["pure"], med["rs"]), labels(med)):
            k = min(k, (W - x0 - 8 - len(lab) * MONO_W) / v)
    return k


def render(res, metric, base="light"):
    groups = {name: res["scenarios"][name][f"median_{metric}"] for name in SCENARIOS if name in res["scenarios"]}
    x0 = 0.5
    k = scale(groups, x0)
    what = "Wall time" if metric == "wall" else "CPU time (process and children)"
    body = [
        text("t", 0, 20, "Same datatrove pipelines, same output, less time"),
        text("s", 0, 40, f"{what} with LocalPipelineExecutor, 1 task, median of {res['reps']} runs. Lower is better."),
    ]
    y = 58
    for cls, name, dx in (("other", "datatrove", 0), ("ours", "datatrove-rs", 98)):
        body.append(f'<rect class="{cls}" x="{dx}" y="{y}" width="12" height="12" rx="2"/>')
        body.append(text("k", dx + 18, y + 11, name))
    y += 36
    top = y - 16
    aria = []
    for name, med in groups.items():
        body.append(text("g", 0, y, SCENARIOS[name]))
        y += 10
        pure_lab, rs_lab = labels(med)
        for cls, v, lab, vcls in (("other", med["pure"], pure_lab, "v"), ("ours", med["rs"], rs_lab, "vo")):
            w = v * k
            body.append(bar(cls, x0, y, w))
            body.append(text(vcls, x0 + w + 8, y + 14.5, lab))
            y += BAR + PAIR_GAP
        aria.append(f"{SCENARIOS[name]}: datatrove {pure_lab}, datatrove-rs {rs_lab}")
        y += GROUP_GAP
    body.insert(2, f'<line class="axis" x1="{x0}" y1="{top}" x2="{x0}" y2="{y - GROUP_GAP - PAIR_GAP + 4}"/>')
    v = res["versions"]
    foot = (f"Measured {res['date']} on {res['cpus']} CPUs: datatrove {v['datatrove']}, datatrove-rs {v['datatrove_rs']}, "
            f"trafilatura-rs {v['trafilatura_rs']}, Python {v['python']}.")
    body.append(text("m", 0, y + 2, foot))
    h = y + 10
    label = escape(f"{what}, lower is better. " + "; ".join(aria) + ".")
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{h}" viewBox="0 0 {W} {h}" role="img" '
            f'aria-label="{label}">\n<title>{label}</title>\n{style(base)}\n' + "\n".join(body) + "\n</svg>\n")


def main(src, out, metric="wall", base="light"):
    with open(src) as f:
        res = json.load(f)
    for name, sc in res["scenarios"].items():
        if not sc["identical_output"]:
            sys.exit(f"{name}: outputs differ between runs; not charting it")
    with open(out, "w") as f:
        f.write(render(res, metric, base))


if __name__ == "__main__":
    main(*sys.argv[1:5])
