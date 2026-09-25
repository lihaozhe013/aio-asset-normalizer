#!/usr/bin/env python3
"""Package the headless CLI as a standalone per-platform archive."""

import argparse
import hashlib
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path


APP_ID = "aio-asset-normalizer"
CLI_BINARY_NAME = "aio-asset-normalizer-cli"
CLI_REFERENCE_NAME = "CLI.md"
CLI_README_NAME = "README.txt"

PROJECT_ROOT = Path(__file__).resolve().parent.parent
CARGO_TOML = PROJECT_ROOT / "Cargo.toml"
TARGET_DIR = PROJECT_ROOT / "target"
CLI_REFERENCE = PROJECT_ROOT / "docs" / CLI_REFERENCE_NAME


def read_version() -> str:
    """Read the package version from Cargo.toml."""
    text = CARGO_TOML.read_text(encoding="utf-8")
    match = re.search(r"(?m)^\s*version\s*=\s*\"([^\"]+)\"", text)
    if match is None:
        raise ValueError(f"Could not read the package version from {CARGO_TOML}")
    return match.group(1)


def ensure_dir(path: Path) -> Path:
    path.mkdir(parents=True, exist_ok=True)
    return path


def run(command: list[object], **kwargs: object) -> subprocess.CompletedProcess[bytes]:
    print(f"  -> {' '.join(str(item) for item in command)}")
    return subprocess.run(command, check=True, **kwargs)


def detect_platform() -> str:
    if sys.platform.startswith("win"):
        return "win"
    if sys.platform == "darwin":
        return "macos"
    if sys.platform.startswith("linux"):
        return "linux"
    print(f"[ERROR] Unsupported platform: {sys.platform}")
    raise SystemExit(1)


def detect_architecture() -> str:
    machine = platform.machine().lower()
    if machine in {"x86_64", "amd64"}:
        return "x86-64"
    if machine in {"aarch64", "arm64"}:
        return "arm64"
    print(f"[ERROR] Unsupported architecture: {machine}")
    raise SystemExit(1)


def executable_name(platform_label: str) -> str:
    if platform_label == "win":
        return f"{CLI_BINARY_NAME}.exe"
    return CLI_BINARY_NAME


def archive_suffix(platform_label: str) -> str:
    return ".zip" if platform_label == "win" else ".tar.gz"


def readme_text(version: str, platform_label: str) -> str:
    binary = executable_name(platform_label)
    return (
        f"{APP_ID} CLI {version} ({platform_label})\n"
        "\n"
        "This archive contains the headless AIO Asset Normalizer CLI.\n"
        "\n"
        f"  {binary} docs --raw      # full command and JSON reference\n"
        f"  {binary} --help\n"
        f"  {binary} glb inspect model.glb\n"
        "\n"
        "stdout is one JSON envelope per run (schema_version 1); logs go to\n"
        "stderr. Exit codes: 0 success, 2 usage, 3 validation, 4 I/O, 5 external\n"
        "tool. Prefer --dry-run before writing and never overwrite without\n"
        "--overwrite.\n"
        "\n"
        f"Full reference: {CLI_REFERENCE_NAME}\n"
    )


def write_archive(archive_path: Path, staging: Path, platform_label: str) -> None:
    names = [executable_name(platform_label), CLI_REFERENCE_NAME, CLI_README_NAME]
    if archive_path.exists():
        archive_path.unlink()
    if platform_label == "win":
        with zipfile.ZipFile(archive_path, "w", zipfile.ZIP_DEFLATED) as archive:
            for name in names:
                archive.write(staging / name, arcname=name)
    else:
        with tarfile.open(archive_path, "w:gz") as archive:
            for name in names:
                archive.add(str(staging / name), arcname=name)


def build_cli_archive(
    version: str,
    output_dir: Path,
    platform_label: str,
    architecture: str,
    profile: str,
    skip_build: bool,
) -> Path:
    binary_source = TARGET_DIR / profile / executable_name(platform_label)

    print("\n" + "=" * 60)
    print(f"  {APP_ID} CLI archive")
    print(
        f"  Version: {version}  Profile: {profile}  "
        f"Platform: {platform_label}  Architecture: {architecture}"
    )
    print("=" * 60 + "\n")

    if skip_build:
        print("[1/4] Skipping build (--skip-build).")
    else:
        print("[1/4] Building CLI executable...")
        command: list[object] = ["cargo", "build"]
        if profile == "release":
            command.append("--release")
        command += ["--bin", CLI_BINARY_NAME]
        run(command, cwd=PROJECT_ROOT)

    if not binary_source.exists():
        print(f"[ERROR] CLI executable not found: {binary_source}")
        print(
            "        Build it first with: "
            f"cargo build --release --bin {CLI_BINARY_NAME}"
        )
        raise SystemExit(1)
    if not CLI_REFERENCE.exists():
        print(f"[ERROR] CLI reference not found: {CLI_REFERENCE}")
        raise SystemExit(1)

    ensure_dir(output_dir)
    archive_name = (
        f"{APP_ID}-cli-{version}-{platform_label}-{architecture}"
        f"{archive_suffix(platform_label)}"
    )
    archive_path = output_dir / archive_name

    print("[2/4] Staging archive contents...")
    with tempfile.TemporaryDirectory(prefix=f"{APP_ID}-cli-") as temporary_directory:
        staging = Path(temporary_directory)
        binary_target = staging / executable_name(platform_label)
        shutil.copy2(binary_source, binary_target)
        binary_target.chmod(0o755)
        shutil.copy2(CLI_REFERENCE, staging / CLI_REFERENCE_NAME)
        (staging / CLI_README_NAME).write_text(
            readme_text(version, platform_label), encoding="utf-8"
        )
        print(f"  [OK] {executable_name(platform_label)}")
        print(f"  [OK] {CLI_REFERENCE_NAME}")
        print(f"  [OK] {CLI_README_NAME}")

        print("[3/4] Writing archive...")
        write_archive(archive_path, staging, platform_label)

    if not archive_path.exists():
        print(f"[ERROR] The archive was not created: {archive_path}")
        raise SystemExit(1)

    digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
    print("[4/4] Archive ready.")
    print("\n" + "=" * 60)
    print(f"  [OK] Archive: {archive_path}")
    print(f"  [OK] Size: {archive_path.stat().st_size / 1024 / 1024:.1f} MB")
    print(f"  [OK] SHA-256: {digest}")
    print("=" * 60 + "\n")
    return archive_path


def main() -> int:
    parser = argparse.ArgumentParser(
        description=f"Package the {APP_ID} headless CLI archive"
    )
    parser.add_argument(
        "--version", "-v", default=None, help="Version (default: read from Cargo.toml)"
    )
    parser.add_argument(
        "--output",
        "-o",
        default="packaging/out",
        help="Output directory (default: packaging/out)",
    )
    parser.add_argument(
        "--platform",
        choices=("win", "macos", "linux"),
        default=None,
        help="Target platform label (default: current platform)",
    )
    parser.add_argument(
        "--arch",
        choices=("x86-64", "arm64"),
        default=None,
        help="Target architecture label (default: current architecture)",
    )
    parser.add_argument("--debug", action="store_true", help="Use the debug executable")
    parser.add_argument(
        "--skip-build", action="store_true", help="Package an existing executable"
    )
    args = parser.parse_args()

    version = args.version or read_version()
    output_dir = Path(args.output).resolve()
    platform_label = args.platform or detect_platform()
    architecture = args.arch or detect_architecture()
    build_cli_archive(
        version,
        output_dir,
        platform_label,
        architecture,
        profile="debug" if args.debug else "release",
        skip_build=args.skip_build,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())