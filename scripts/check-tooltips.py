#!/usr/bin/env python3
"""Fails when an icon button in the Mac app has no tooltip.

    scripts/check-tooltips.py [FILE...]

A toolbar or inline button drawn as an icon shows nothing but the icon, so
Reply, Reply All and Forward look alike until clicked. Every
`Button(..., systemImage: ...)` gets a `.help(...)` with the words of its
label. A button inside a context menu, a swipe action, a `Menu` or a view
named `...Menu` shows its words already and needs none, so those are left
alone. With no FILE, every Swift file under macos/Iris is read.

A button the rule gets wrong carries `// no tooltip: <why>` on its line.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
BUTTON = re.compile(r"\bButton\(.*\bsystemImage:")
# Blocks whose buttons show their words: a menu, a context menu, swipes,
# and a view made to be a menu's contents, which the app names `...Menu`.
WORDED = re.compile(r"(\.contextMenu|\.swipeActions|\bMenu|\bstruct \w+Menu\b[^{]*)\s*(\([^)]*\))?\s*\{")
# How far below a button its modifiers can sit.
REACH = 12


def depth_change(line: str) -> int:
    """The braces a line opens minus those it closes, outside strings."""
    line = re.sub(r'"(\\.|[^"\\])*"', '""', line)
    return line.count("{") - line.count("}")


def missing(path: pathlib.Path) -> list[tuple[int, str]]:
    lines = path.read_text(encoding="utf-8").split("\n")
    found = []
    depth = 0
    # The depths at which a worded block was opened, innermost last.
    worded: list[int] = []
    for number, line in enumerate(lines):
        while worded and depth <= worded[-1]:
            worded.pop()
        if BUTTON.search(line) and not worded and "no tooltip:" not in line:
            if not has_help(lines, number):
                found.append((number + 1, line.strip()))
        if WORDED.search(line):
            worded.append(depth)
        depth += depth_change(line)
    return found


def has_help(lines: list[str], start: int) -> bool:
    """Whether the button at `start` is given `.help` before the next view
    begins: its own lines and the modifiers chained under them."""
    depth = 0
    for offset, line in enumerate(lines[start : start + REACH]):
        if offset > 0 and depth == 0 and not line.lstrip().startswith("."):
            return False
        if ".help(" in line:
            return True
        depth += depth_change(line)
    return False


def main() -> int:
    files = [pathlib.Path(a) for a in sys.argv[1:]] or sorted((ROOT / "macos/Iris").glob("*.swift"))
    problems = [(f, n, text) for f in files for n, text in missing(f)]
    for f, n, text in problems:
        name = f.relative_to(ROOT) if f.is_absolute() else f
        print(f"{name}:{n}: icon button without .help: {text[:100]}")
    if problems:
        print(f"{len(problems)} icon buttons have no tooltip; give each a .help with its label.")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
