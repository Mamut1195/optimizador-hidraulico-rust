"""Lightweight repository secret-pattern scanner for CI.

This is intentionally dependency-free so CI can run it before installing any
extra tools. It scans tracked files when executed inside a Git checkout and
falls back to walking the working tree otherwise.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]

SKIP_DIRS = {
    ".git",
    "target",
    ".pytest_cache",
    "__pycache__",
}

SKIP_SUFFIXES = {
    ".exe",
    ".dll",
    ".pdb",
    ".rlib",
    ".png",
    ".jpg",
    ".jpeg",
    ".gif",
    ".webp",
    ".ico",
    ".pdf",
    ".zip",
}

PATTERNS = [
    ("aws_access_key", re.compile(r"\bAKIA[0-9A-Z]{16}\b")),
    ("github_pat", re.compile(r"\bgh[pousr]_[A-Za-z0-9_]{20,}\b")),
    ("private_key", re.compile(r"-----BEGIN (?:RSA |DSA |EC |OPENSSH )?PRIVATE KEY-----")),
    (
        "named_secret_assignment",
        re.compile(
            r"(?i)\b(?:api[_-]?key|secret|password|token|private[_-]?key)\b"
            r"\s*[:=]\s*['\"][^'\"\s]{12,}['\"]"
        ),
    ),
]

ALLOWLIST = [
    re.compile(r"design-tokens", re.IGNORECASE),
    re.compile(r"token budget", re.IGNORECASE),
    re.compile(r"placeholder|example|sample|fixture", re.IGNORECASE),
]


def tracked_files() -> list[Path]:
    try:
        result = subprocess.run(
            ["git", "ls-files"],
            cwd=ROOT,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError):
        return []

    return [ROOT / line for line in result.stdout.splitlines() if line.strip()]


def walked_files() -> list[Path]:
    paths: list[Path] = []
    for current_root, dirnames, filenames in os.walk(ROOT):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for filename in filenames:
            paths.append(Path(current_root) / filename)
    return paths


def should_scan(path: Path) -> bool:
    parts = set(path.relative_to(ROOT).parts)
    if parts & SKIP_DIRS:
        return False
    if path.suffix.lower() in SKIP_SUFFIXES:
        return False
    return True


def is_allowlisted(line: str) -> bool:
    return any(pattern.search(line) for pattern in ALLOWLIST)


def main() -> int:
    candidates = tracked_files() or walked_files()
    findings: list[str] = []

    for path in candidates:
        if not path.exists() or not should_scan(path):
            continue

        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue

        rel = path.relative_to(ROOT)
        for line_no, line in enumerate(text.splitlines(), start=1):
            if is_allowlisted(line):
                continue
            for label, pattern in PATTERNS:
                if pattern.search(line):
                    findings.append(f"{rel}:{line_no}: {label}")

    if findings:
        print("Potential secrets found:", file=sys.stderr)
        for finding in findings:
            print(f"  {finding}", file=sys.stderr)
        return 1

    print("No secret patterns found.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
