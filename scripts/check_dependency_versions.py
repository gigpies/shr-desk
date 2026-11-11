#!/usr/bin/env python3
"""Report registry version drift without resolving the frozen panel registry.

Read-only inventory, not a resolver, security audit, or automatic upgrade.
"""
import argparse
import json
from pathlib import Path
import re
import tomllib
import urllib.parse
import urllib.request


def dependencies(root):
    names = set()
    manifests = [root / "Cargo.toml", *sorted((root / "vendor").glob("*/Cargo.toml"))]
    for manifest in manifests:
        data = tomllib.loads(manifest.read_text())
        tables = [data, *data.get("target", {}).values()]
        for table in tables:
            for group in ("dependencies", "build-dependencies", "dev-dependencies"):
                for name, spec in table.get(group, {}).items():
                    if isinstance(spec, dict):
                        if any(key in spec for key in ("registry", "git", "path", "workspace")):
                            continue
                        name = spec.get("package", name)
                    if not re.fullmatch(r"[A-Za-z0-9_-]+", name):
                        raise ValueError("invalid crate name")
                    names.add(name)
    locked = tomllib.loads((root / "Cargo.lock").read_text())["package"]
    return {name: sorted({p["version"] for p in locked if p["name"] == name
                         and p.get("source", "").startswith("registry+https://github.com/rust-lang/crates.io-index")})
            for name in sorted(names)}


def latest(name):
    request = urllib.request.Request(
        "https://crates.io/api/v1/crates/" + urllib.parse.quote(name, safe=""),
        headers={"User-Agent": "GigPies-dependency-inventory (https://github.com/gigpies)"},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        version = json.load(response)["crate"]["max_stable_version"]
    if not isinstance(version, str) or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:\+[A-Za-z0-9.-]+)?", version):
        raise ValueError("missing or invalid stable version for " + name)
    return version


def report(packages, lookup=latest):
    lines = ["# Cargo dependency inventory", "",
             "Direct crates.io dependencies from the application and frozen vendor manifests.",
             "Versions are observations, not compatibility or upgrade approval.", "",
             "| Crate | Locked | Latest stable | Review |",
             "| --- | --- | --- | --- |"]
    for name, versions in packages.items():
        newest = lookup(name)
        review = "current" if versions == [newest] else "review available versions"
        lines.append(f"| {name} | {', '.join(versions) or 'not locked'} | {newest} | {review} |")
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    print(report(dependencies(args.root)), end="")
