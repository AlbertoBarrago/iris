#!/usr/bin/env python3
"""Makes an Iris.app carry its own GTK, so it runs on a Mac without Homebrew.

    scripts/macos-bundle-libs.py path/to/Iris.app

Copies every Homebrew library the app loads, and the libraries those load,
into Contents/Frameworks, and points each reference at the copy through
@executable_path, so the dynamic loader never looks in /opt/homebrew. The
same goes for gdk-pixbuf's image loaders, which GTK opens at run time. GTK's
data follows into Contents/Resources: the Adwaita and hicolor icon themes,
the compiled GSettings schemas, a fontconfig config and the toolkit's own
translations. app/src/macos_bundle.rs points GTK at them as the app starts.

macos-dmg.sh runs this on a copy of the bundle; the bundle that
macos-install.sh puts in /Applications keeps loading Homebrew's GTK.
"""

import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

HOMEBREW = "/opt/homebrew"
FRAMEWORKS_REF = "@executable_path/../Frameworks"
# The languages Iris speaks, and so the toolkit catalogues worth carrying.
LANGUAGES = ["en_GB", "it", "it_IT", "pt", "pt_PT"]
TOOLKIT_DOMAINS = ["gtk40", "glib20", "libadwaita"]


def run(*args: str) -> str:
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout


def references(binary: Path) -> list[str]:
    """The libraries `binary` names, as written in it."""
    lines = run("otool", "-L", str(binary)).splitlines()[1:]
    return [line.strip().split(" (")[0] for line in lines]


def rpaths(binary: Path) -> list[str]:
    out = run("otool", "-l", str(binary))
    return re.findall(r"cmd LC_RPATH\n\s+cmdsize \d+\n\s+path (\S+)", out)


def resolve(reference: str, by: Path) -> Path | None:
    """Where a Homebrew `reference` made by `by` points, or None for one
    the system provides."""
    if reference.startswith(HOMEBREW):
        return Path(reference)
    if reference.startswith("@loader_path/"):
        return (by.resolve().parent / reference.removeprefix("@loader_path/")).resolve()
    if reference.startswith("@rpath/"):
        name = reference.removeprefix("@rpath/")
        for rpath in rpaths(by):
            base = rpath.replace("@loader_path", str(by.resolve().parent))
            candidate = Path(base) / name
            if candidate.exists():
                return candidate.resolve()
        found = list(Path(HOMEBREW, "lib").glob(name))
        return found[0].resolve() if found else None
    return None


def gather(roots: list[Path]) -> dict[str, Path]:
    """Every Homebrew library `roots` reach, by file name."""
    found: dict[str, Path] = {}
    todo = list(roots)
    while todo:
        binary = todo.pop()
        for reference in references(binary):
            target = resolve(reference, binary)
            if target is None or target.name in found:
                continue
            if not target.exists():
                sys.exit(f"{binary} needs {reference}, which is not on this Mac")
            found[target.name] = target.resolve()
            todo.append(target.resolve())
    return found


def relink(binary: Path, names: set[str], own_id: str | None = None) -> None:
    """Points each of `binary`'s references to a bundled library at the
    copy, and gives a library its own id."""
    args: list[str] = []
    if own_id:
        args += ["-id", own_id]
    for reference in references(binary):
        name = Path(reference).name
        if name in names and not reference.startswith(FRAMEWORKS_REF):
            args += ["-change", reference, f"{FRAMEWORKS_REF}/{name}"]
    if args:
        run("install_name_tool", *args, str(binary))


def copy_tree(source: Path, target: Path) -> None:
    if target.exists():
        shutil.rmtree(target)
    shutil.copytree(source, target, symlinks=False)


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    app = Path(sys.argv[1]).resolve()
    executable = app / "Contents/MacOS/iris"
    frameworks = app / "Contents/Frameworks"
    resources = app / "Contents/Resources"
    if not executable.is_file():
        sys.exit(f"No {executable}")

    pixbuf_source = Path(HOMEBREW, "lib/gdk-pixbuf-2.0/2.10.0/loaders")
    loaders = sorted(p.resolve() for p in pixbuf_source.glob("*.so"))
    libraries = gather([executable, *loaders])
    names = set(libraries)

    if frameworks.exists():
        shutil.rmtree(frameworks)
    frameworks.mkdir(parents=True)
    for name, source in sorted(libraries.items()):
        copy = frameworks / name
        shutil.copy2(source, copy)
        copy.chmod(0o755)
        relink(copy, names, own_id=f"{FRAMEWORKS_REF}/{name}")
    relink(executable, names)

    loader_dir = resources / "lib/gdk-pixbuf-2.0/2.10.0/loaders"
    if loader_dir.parent.exists():
        shutil.rmtree(loader_dir.parent)
    loader_dir.mkdir(parents=True)
    for loader in loaders:
        copy = loader_dir / loader.name
        shutil.copy2(loader, copy)
        copy.chmod(0o755)
        # librsvg's SVG loader is a library, not a plain module, and so
        # carries an id of its own, which must stop naming Homebrew too.
        is_library = run("otool", "-D", str(copy)).strip().count("\n") > 0
        own_id = f"@executable_path/../Resources/lib/gdk-pixbuf-2.0/2.10.0/loaders/{copy.name}"
        relink(copy, names, own_id=own_id if is_library else None)
    # The cache names each loader by its full path. It is written for the
    # Homebrew copies, then their folder becomes a placeholder the app
    # fills in with wherever the bundle is (macos_bundle.rs).
    cache = run("gdk-pixbuf-query-loaders", *map(str, loaders))
    for folder in {str(pixbuf_source), str(pixbuf_source.resolve()), *(str(l.parent) for l in loaders)}:
        cache = cache.replace(folder, "@BUNDLE_LOADERS@")
    (loader_dir.parent / "loaders.cache.in").write_text(cache)

    share = resources / "share"
    share.mkdir(parents=True, exist_ok=True)
    for theme in ["Adwaita", "hicolor"]:
        copy_tree(Path(HOMEBREW, "share/icons", theme).resolve(), share / "icons" / theme)
    schemas = share / "glib-2.0/schemas"
    schemas.mkdir(parents=True, exist_ok=True)
    shutil.copy2(Path(HOMEBREW, "share/glib-2.0/schemas/gschemas.compiled"), schemas)
    for language in LANGUAGES:
        for domain in TOOLKIT_DOMAINS:
            source = Path(HOMEBREW, "share/locale", language, "LC_MESSAGES", f"{domain}.mo")
            if source.exists():
                target = share / "locale" / language / "LC_MESSAGES"
                target.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source.resolve(), target / source.name)

    # fontconfig reads its config from Homebrew's prefix unless told
    # otherwise. Pango draws with Core Text on macOS, so this only has to
    # name the system's font folders and a cache Iris may write to.
    fonts = resources / "etc/fonts"
    fonts.mkdir(parents=True, exist_ok=True)
    (fonts / "fonts.conf").write_text(
        """<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
  <dir>/System/Library/Fonts</dir>
  <dir>/Library/Fonts</dir>
  <dir>~/Library/Fonts</dir>
  <cachedir>~/Library/Caches/io.github.AlbertoBarrago.Iris/fontconfig</cachedir>
</fontconfig>
"""
    )

    leftover = [
        str(binary)
        for binary in [executable, *frameworks.iterdir(), *loader_dir.iterdir()]
        if any(r.startswith(HOMEBREW) for r in references(binary))
    ]
    if leftover:
        sys.exit("Still pointing at Homebrew:\n  " + "\n  ".join(leftover))
    print(f"Bundled {len(libraries)} libraries and {len(loaders)} image loaders into {app.name}")


if __name__ == "__main__":
    main()
