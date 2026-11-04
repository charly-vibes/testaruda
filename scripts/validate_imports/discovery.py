"""Language auto-detection and source-file discovery."""

import json
import os
import subprocess
from pathlib import Path

from validate_imports.base import LanguageBackend

# (config-file markers), language — checked in priority order
_CONFIG_MARKERS: list[tuple[tuple[str, ...], str]] = [
    (("Cargo.toml",), "rust"),
    (("Project.toml",), "julia"),
    (("package.json",), "typescript"),
    (("deps.edn", "project.clj"), "clojure"),
    (("pyproject.toml", "setup.py", "requirements.txt"), "python"),
]
_EXTENSION_FALLBACK: list[tuple[str, str]] = [
    (".py", "python"),
    (".rs", "rust"),
]


def detect_language(repo_root: str) -> str | None:
    """Auto-detect project language from config files."""
    files = os.listdir(repo_root)
    for markers, lang in _CONFIG_MARKERS:
        if any(marker in files for marker in markers):
            return lang
    if any(f.endswith(".jl") for f in files):
        return "julia"
    # Fallback: check for source files
    exts = {Path(f).suffix for f in files}
    for ext, lang in _EXTENSION_FALLBACK:
        if ext in exts:
            return lang
    return None


def find_source_files(repo_root: str, backend: LanguageBackend) -> list[tuple[str, str]]:
    """Find all source files and return [(rel_path, abs_path), ...]."""
    files: list[tuple[str, str]] = []
    excludes = backend.exclude_dirs()
    repo = Path(repo_root).resolve()
    for f in repo.rglob("*"):
        if f.suffix not in backend.source_extensions:
            continue
        rel = str(f.relative_to(repo))
        parts = rel.split("/")
        if any(p in excludes for p in parts):
            continue
        if any(p.startswith(".") and p != "." for p in parts):
            continue
        files.append((rel, str(f)))
    return sorted(files)


def run_adapter(adapter: str, cmd: dict, cwd: str) -> dict | None:
    """Run a single adapter command and return parsed JSON."""
    try:
        result = subprocess.run(
            [adapter], input=json.dumps(cmd), capture_output=True,
            text=True, timeout=30, cwd=cwd,
        )
        if result.returncode != 0:
            return None
        first = result.stdout.strip().split("\n")[0]
        return json.loads(first)
    except Exception:
        return None
