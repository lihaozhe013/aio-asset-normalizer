#!/usr/bin/env python3
"""Verify the MCP-only dependency boundary and agent skill resources."""
import json
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent.parent
FORBIDDEN = {"egui", "egui-phosphor", "three-d", "three-d-asset", "winit", "rfd", "arboard"}


def main():
    tree = subprocess.check_output(
        ["cargo", "tree", "--locked", "--no-default-features", "--features", "mcp", "--edges", "normal", "--prefix", "none", "--format", "{p}"],
        cwd=ROOT, text=True,
    )
    packages = {line.split()[0] for line in tree.splitlines() if line.strip()}
    unexpected = packages & FORBIDDEN
    if unexpected:
        raise SystemExit(f"Desktop dependencies in MCP-only build: {sorted(unexpected)}")
    skill = ROOT / "skills" / "aio-asset-normalizer"
    text = (skill / "SKILL.md").read_text(encoding="utf-8")
    if not text.startswith("---\n") or "name: aio-asset-normalizer" not in text:
        raise SystemExit("Missing skill frontmatter")
    for name in ["glb", "bvh", "retarget", "converter"]:
        reference = skill / "references" / f"{name}.md"
        if not reference.is_file() or f"references/{name}.md" not in text:
            raise SystemExit(f"Missing skill reference: {name}")
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=ROOT, text=True,
    ))
    package = metadata["packages"][0]
    if "mcp" not in package["features"]:
        raise SystemExit("MCP feature is missing")
    binaries = {
        target["name"]
        for target in package["targets"]
        if "bin" in target["kind"]
    }
    expected = {"aio-asset-normalizer", "aio-asset-normalizer-mcp"}
    if binaries != expected:
        raise SystemExit(f"Unexpected binary targets: {sorted(binaries)}")
    print("MCP-only dependency boundary and agent skill resources verified.")


if __name__ == "__main__":
    main()
