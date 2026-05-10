#!/usr/bin/env python3
"""Prepare and extract Keep a Changelog release sections."""

from __future__ import annotations

import argparse
import datetime as dt
import re
from pathlib import Path


EMPTY_UNRELEASED = """## [Unreleased]

### Added

### Changed

### Fixed"""


def _release_heading(version: str) -> str:
    return rf"## \[{re.escape(version)}\](?: - \d{{4}}-\d{{2}}-\d{{2}})?"


def _find_section(text: str, heading_pattern: str) -> re.Match[str]:
    pattern = re.compile(
        rf"(?ms)^({heading_pattern})\s*\n(?P<body>.*?)(?=^## \[|\Z)"
    )
    match = pattern.search(text)
    if not match:
        raise SystemExit(f"Could not find changelog section matching: {heading_pattern}")
    return match


def _has_release_content(body: str) -> bool:
    for line in body.splitlines():
        stripped = line.strip()
        if stripped and not stripped.startswith("###"):
            return True
    return False


def prepare(path: Path, version: str, release_date: str) -> None:
    text = path.read_text()
    if re.search(rf"(?m)^## \[{re.escape(version)}\](?: - |$)", text):
        return

    unreleased = _find_section(text, r"## \[Unreleased\]")
    body = unreleased.group("body").strip()
    if not _has_release_content(body):
        raise SystemExit("Refusing to create an empty release changelog section")

    release_section = f"{EMPTY_UNRELEASED}\n\n## [{version}] - {release_date}\n\n{body}\n\n"
    updated = text[: unreleased.start()] + release_section + text[unreleased.end() :]
    path.write_text(updated)


def extract(path: Path, version: str) -> str:
    text = path.read_text()
    release = _find_section(text, _release_heading(version))
    body = release.group("body").strip()
    if not body:
        raise SystemExit(f"Release {version} has no changelog content")
    return body


def main() -> None:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    prepare_parser = subparsers.add_parser("prepare")
    prepare_parser.add_argument("--file", default="CHANGELOG.md", type=Path)
    prepare_parser.add_argument("--version", required=True)
    prepare_parser.add_argument("--date", default=dt.date.today().isoformat())

    extract_parser = subparsers.add_parser("extract")
    extract_parser.add_argument("--file", default="CHANGELOG.md", type=Path)
    extract_parser.add_argument("--version", required=True)

    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args.file, args.version, args.date)
    elif args.command == "extract":
        print(extract(args.file, args.version))


if __name__ == "__main__":
    main()
