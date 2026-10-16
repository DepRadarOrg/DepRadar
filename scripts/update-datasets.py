#!/usr/bin/env python3
"""Regenerate the popular-package datasets in data/popular-packages/.

Fetches the current most-downloaded packages for npm, PyPI and crates.io from
public download-stat sources and rewrites each `<registry>-top1000.json` as a
sorted, de-duplicated JSON array of lowercase names. A file is only rewritten
when its content changes, so running this twice in a row leaves no diff.

Usage:
    python3 scripts/update-datasets.py            # all registries
    python3 scripts/update-datasets.py npm pypi   # just these

Sources and caveats are documented in data/popular-packages/README.md.
Standard library only; needs Python 3.9+.
"""

import argparse
import json
import re
import sys
import time
import urllib.request
from pathlib import Path

TOP_N = 1000
DATA_DIR = Path(__file__).resolve().parent.parent / "data" / "popular-packages"
USER_AGENT = "DepRadar dataset updater (https://github.com/DepRadarOrg/DepRadar)"

NPM_SOURCE = "https://raw.githubusercontent.com/wooorm/npm-high-impact/main/lib/top-download.js"
PYPI_SOURCE = "https://hugovk.dev/top-pypi-packages/top-pypi-packages.min.json"
CRATES_SOURCE = "https://crates.io/api/v1/crates?sort=recent-downloads&per_page=100&page={page}"

# Name rules, matching the dataset validation test in src/typosquat/dataset.rs.
NPM_PART = r"[a-z0-9~-][a-z0-9._~-]*"
NPM_NAME = re.compile(rf"(@{NPM_PART}/)?{NPM_PART}")
PYPI_NAME = re.compile(r"[a-z0-9]([a-z0-9._-]*[a-z0-9])?")
CRATES_NAME = re.compile(r"[a-z][a-z0-9_-]*")


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read().decode("utf-8")


def npm_ranked():
    """npm names, most-downloaded first.

    npm has no official "top packages" endpoint. npm-high-impact ranks every
    package with over a million weekly downloads by its npm download count
    and publishes the result as a JS array of string literals.
    """
    return re.findall(r"^\s*'([^']+)',?$", fetch(NPM_SOURCE), flags=re.M)


def pypi_ranked():
    """PyPI names (PEP 503 normalised), most-downloaded over 30 days first."""
    rows = json.loads(fetch(PYPI_SOURCE))["rows"]
    rows.sort(key=lambda row: row["download_count"], reverse=True)
    return [row["project"] for row in rows]


def crates_ranked():
    """crates.io names, most-downloaded over 90 days first.

    The API serves 100 crates per page. Fetching one page per second keeps
    within the crates.io crawler policy, which asks for at most one request
    per second and an identifying User-Agent.
    """
    names = []
    for page in range(1, TOP_N // 100 + 3):
        if page > 1:
            time.sleep(1)
        crates = json.loads(fetch(CRATES_SOURCE.format(page=page)))["crates"]
        names.extend(crate["name"] for crate in crates)
        if len(set(names)) >= TOP_N or not crates:
            break
    return names


def pypi_identity(name):
    return re.sub(r"[-_.]+", "-", name)


REGISTRIES = {
    "npm": (npm_ranked, lambda name: NPM_NAME.fullmatch(name) and len(name) <= 214, str),
    "pypi": (pypi_ranked, PYPI_NAME.fullmatch, pypi_identity),
    "crates": (crates_ranked, lambda name: CRATES_NAME.fullmatch(name) and len(name) <= 64, str),
}


def top_names(ranked, is_valid, identity):
    """The TOP_N highest-ranked valid, distinct names, sorted alphabetically."""
    chosen = {}
    skipped = []
    for raw in ranked:
        name = raw.strip().lower()
        if not is_valid(name):
            skipped.append(raw)
            continue
        chosen.setdefault(identity(name), name)
        if len(chosen) == TOP_N:
            break
    if len(chosen) < TOP_N:
        sys.exit(f"source only yielded {len(chosen)} valid names, expected {TOP_N}")
    if skipped:
        print(f"  skipped {len(skipped)} invalid name(s): {', '.join(skipped[:5])}")
    return sorted(chosen.values())


def update(registry):
    ranked, is_valid, identity = REGISTRIES[registry]
    path = DATA_DIR / f"{registry}-top1000.json"
    print(f"{registry}: fetching")
    names = top_names(ranked(), is_valid, identity)
    content = json.dumps(names, indent=2) + "\n"

    old = path.read_text(encoding="utf-8") if path.exists() else "[]"
    old_names = set(json.loads(old))
    if content == old:
        print(f"{registry}: unchanged ({path.name})")
        return
    path.write_text(content, encoding="utf-8")
    added = len(set(names) - old_names)
    removed = len(old_names - set(names))
    print(f"{registry}: wrote {path.name} (+{added} -{removed})")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "registries",
        nargs="*",
        metavar="REGISTRY",
        help=f"registries to update: {', '.join(REGISTRIES)} (default: all)",
    )
    registries = parser.parse_args().registries or list(REGISTRIES)
    unknown = [r for r in registries if r not in REGISTRIES]
    if unknown:
        parser.error(f"unknown registry {unknown[0]!r} (choose from {', '.join(REGISTRIES)})")
    for registry in registries:
        update(registry)


if __name__ == "__main__":
    main()
