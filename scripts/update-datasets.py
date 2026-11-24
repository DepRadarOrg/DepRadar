#!/usr/bin/env python3
"""Regenerate the popular-package datasets in data/popular-packages/.

Fetches the current most-downloaded packages for npm, PyPI, crates.io,
RubyGems, Maven Central, Go modules and Packagist from public download-stat
sources and rewrites each `<registry>-top1000.json` as a de-duplicated JSON
array of lowercase names, most-downloaded first (the typo-squat scanner uses
that order as a popularity rank). A file is only rewritten when its content
changes, so running this twice in a row leaves no diff.

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
import urllib.parse
import urllib.request
from pathlib import Path

TOP_N = 1000
# npm gets a larger reference set so squats of mid-popularity packages are caught.
NPM_TOP_N = 5000
DATA_DIR = Path(__file__).resolve().parent.parent / "data" / "popular-packages"
METADATA_PATH = DATA_DIR / "generated-at.json"
USER_AGENT = "DepRadar dataset updater (https://github.com/DepRadarOrg/DepRadar)"

NPM_SOURCE = "https://raw.githubusercontent.com/wooorm/npm-high-impact/main/lib/top-download.js"
PYPI_SOURCE = "https://hugovk.dev/top-pypi-packages/top-pypi-packages.min.json"
CRATES_SOURCE = "https://crates.io/api/v1/crates?sort=recent-downloads&per_page=100&page={page}"
RUBYGEMS_SOURCE = "https://bestgems.org/total?page={page}"
MAVEN_SOURCE = "https://api.deps.dev/v3alpha/query?system=MAVEN&pageSize=100&pageToken={token}"
GO_SOURCE = "https://proxy.golang.org/index?since={since}"
PACKAGIST_SOURCE = "https://packagist.org/explore/popular.json?page={page}"

# Name rules, matching the dataset validation test in src/typosquat/dataset.rs.
NPM_PART = r"[a-z0-9~-][a-z0-9._~-]*"
NPM_NAME = re.compile(rf"(@{NPM_PART}/)?{NPM_PART}")
PYPI_NAME = re.compile(r"[a-z0-9]([a-z0-9._-]*[a-z0-9])?")
CRATES_NAME = re.compile(r"[a-z][a-z0-9_-]*")
RUBYGEMS_NAME = re.compile(r"[a-z0-9][a-z0-9._-]*")
MAVEN_NAME = re.compile(r"[a-z0-9._-]+:[a-z0-9._-]+")
GO_NAME = re.compile(r"[a-z0-9][a-z0-9._~-]*(/[a-z0-9][a-z0-9._~-]*)*")
PACKAGIST_NAME = re.compile(r"[a-z0-9]([a-z0-9._-]*[a-z0-9])?/[a-z0-9]([a-z0-9._-]*[a-z0-9])?")


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


def rubygems_ranked():
    """RubyGems names, most-downloaded (all time) first.

    rubygems.org's own stats page only lists the top 100 gems and its old
    top-downloads API is gone, so this reads BestGems' total-downloads
    ranking, which is built from the rubygems.org download counts and serves
    20 gems per page. Pages are fetched one per second.
    """
    names = []
    for page in range(1, TOP_N // 20 + 6):
        if page > 1:
            time.sleep(1)
        gems = re.findall(r'<td><a href="/gems/([^"]+)">', fetch(RUBYGEMS_SOURCE.format(page=page)))
        names.extend(gems)
        if len(set(names)) >= TOP_N or not gems:
            break
    return names


def maven_ranked():
    """Maven Central `group:artifact` names, most-dependent first.

    deps.dev's query endpoint ranks Maven packages by their dependent count
    (how many other packages depend on them) and serves 100 per page with an
    opaque page token. Pages are fetched one per second to stay polite.
    """
    names = []
    token = ""
    while True:
        if token:
            time.sleep(1)
        payload = json.loads(fetch(MAVEN_SOURCE.format(token=urllib.parse.quote(token))))
        results = payload.get("results", [])
        for result in results:
            version = result.get("version", {})
            names.append(f"{version.get('versionKey', {}).get('name', '')}")
        token = payload.get("nextPageToken", "")
        if len(set(names)) >= TOP_N or not token or not results:
            break
    return names


def go_ranked():
    """Go module paths, most-imported first.

    deps.dev and pkg.go.dev do not expose a ranked import-count list, so this
    reads the Go module proxy index, which lists module paths in the order
    they were first published. That is a rough proxy for popularity; the
    index is fetched in ascending `since` windows of 2000 entries each.
    """
    names = []
    since = ""
    for _ in range(TOP_N // 2000 + 6):
        url = GO_SOURCE.format(since=since) if since else GO_SOURCE.format(since="")
        lines = fetch(url).splitlines()
        if not lines:
            break
        for line in lines:
            try:
                entry = json.loads(line)
            except json.JSONDecodeError:
                continue
            path = entry.get("Path")
            if path:
                names.append(path)
        since = lines[-1]
        if len(set(names)) >= TOP_N:
            break
    return names


def packagist_ranked():
    """Packagist names (vendor/package), most-downloaded first.

    packagist.org's explore/popular endpoint serves the popular packages in
    pages of 100, ordered by download count.
    """
    names = []
    for page in range(1, TOP_N // 100 + 3):
        if page > 1:
            time.sleep(1)
        packages = json.loads(fetch(PACKAGIST_SOURCE.format(page=page)))["packages"]
        names.extend(package["name"] for package in packages)
        if len(set(names)) >= TOP_N or not packages:
            break
    return names


def pypi_identity(name):
    return re.sub(r"[-_.]+", "-", name)


REGISTRIES = {
    "npm": (npm_ranked, lambda name: NPM_NAME.fullmatch(name) and len(name) <= 214, str),
    "pypi": (pypi_ranked, PYPI_NAME.fullmatch, pypi_identity),
    "crates": (crates_ranked, lambda name: CRATES_NAME.fullmatch(name) and len(name) <= 64, str),
    "rubygems": (rubygems_ranked, RUBYGEMS_NAME.fullmatch, str),
    "maven": (maven_ranked, MAVEN_NAME.fullmatch, str),
    "go": (go_ranked, lambda name: GO_NAME.fullmatch(name) and len(name) <= 255, str),
    "packagist": (
        packagist_ranked,
        lambda name: PACKAGIST_NAME.fullmatch(name) and len(name) <= 214,
        str,
    ),
}

# Per-registry reference-set size; registries not listed use TOP_N.
REGISTRY_TOP_N = {"npm": NPM_TOP_N}


def top_names(ranked, is_valid, identity, limit):
    """The `limit` highest-ranked valid, distinct names, most-downloaded first."""
    chosen = {}
    skipped = []
    for raw in ranked:
        name = raw.strip().lower()
        if not is_valid(name):
            skipped.append(raw)
            continue
        chosen.setdefault(identity(name), name)
        if len(chosen) == limit:
            break
    if len(chosen) < limit:
        sys.exit(f"source only yielded {len(chosen)} valid names, expected {limit}")
    if skipped:
        print(f"  skipped {len(skipped)} invalid name(s): {', '.join(skipped[:5])}")
    return list(chosen.values())


def read_metadata():
    """The registry -> generated_at mapping, or {} when absent."""
    if not METADATA_PATH.exists():
        return {}
    return json.loads(METADATA_PATH.read_text(encoding="utf-8"))


def write_metadata(metadata):
    """Rewrite generated-at.json, sorted by registry, only when it changes."""
    content = json.dumps(dict(sorted(metadata.items())), indent=2) + "\n"
    old = METADATA_PATH.read_text(encoding="utf-8") if METADATA_PATH.exists() else ""
    if content == old:
        return
    METADATA_PATH.write_text(content, encoding="utf-8")


def update(registry, metadata):
    ranked, is_valid, identity = REGISTRIES[registry]
    limit = REGISTRY_TOP_N.get(registry, TOP_N)
    path = DATA_DIR / f"{registry}-top{limit}.json"
    print(f"{registry}: fetching top {limit}")
    names = top_names(ranked(), is_valid, identity, limit)
    content = json.dumps(names, indent=2) + "\n"

    old = path.read_text(encoding="utf-8") if path.exists() else "[]"
    old_names = set(json.loads(old))
    if content == old:
        print(f"{registry}: unchanged ({path.name})")
        return
    path.write_text(content, encoding="utf-8")
    metadata[registry] = time.strftime("%Y-%m-%d", time.gmtime())
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
    metadata = read_metadata()
    for registry in registries:
        update(registry, metadata)
    write_metadata(metadata)


if __name__ == "__main__":
    main()
