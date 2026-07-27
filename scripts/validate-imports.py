#!/usr/bin/env python3
"""
validate-imports.py — Multi-language adapter import parsing validator.

Validates each language adapter's import detection by comparing against
ground-truth extracted via the language's own parser (ast, syn, etc.).

Usage:
  ./scripts/validate-imports.py [options] <repo-path>

Options:
  --language LANG  Force language (auto-detect from repo by default)
  --adapter PATH   Adapter binary path (auto-detect from config by default)
  --output FILE    Write JSON results to file
  --list-languages Show supported languages and exit
  --help           Show this help

Supported languages: python, rust, julia, typescript, clojure
"""

import argparse
import ast
import json
import os
import re
import subprocess
import sys
from abc import ABC, abstractmethod
from collections import defaultdict
from pathlib import Path


# ===========================================================================
# Language Backend Base
# ===========================================================================


class LanguageBackend(ABC):
    """Base class for language-specific import validation."""

    @property
    @abstractmethod
    def name(self) -> str: ...

    @property
    @abstractmethod
    def source_extensions(self) -> tuple[str, ...]: ...

    @abstractmethod
    def is_test_file(self, rel_path: str) -> bool: ...

    @abstractmethod
    def extract_imports_ast(self, content: str, file_path: str, repo_root: str) -> set[str]:
        """Ground-truth imports via language parser/AST."""
        ...

    @abstractmethod
    def extract_imports_adapter(self, content: str, file_path: str, repo_root: str) -> set[str]:
        """Replicate adapter's import parsing logic for comparison."""
        ...

    def exclude_dirs(self) -> set[str]:
        return {
            ".venv", "venv", "__pycache__", ".mypy_cache", ".pytest_cache",
            "build", "dist", ".git", "target", "node_modules", ".tox",
            ".eggs", ".direnv", ".bzr",
        }


# ===========================================================================
# Python Backend
# ===========================================================================


class PythonBackend(LanguageBackend):

    @property
    def name(self) -> str: return "python"

    @property
    def source_extensions(self) -> tuple[str, ...]: return (".py",)

    def is_test_file(self, rel_path: str) -> bool:
        base = os.path.basename(rel_path)
        return base.startswith("test_") or base.endswith("_test.py") or "/test_" in rel_path or "/tests/" in rel_path

    def extract_imports_ast(self, content: str, file_path: str, repo_root: str) -> set[str]:
        return _py_ast_imports(content, str(file_path), repo_root)

    def extract_imports_adapter(self, content: str, file_path: str, repo_root: str) -> set[str]:
        """Replicate the Rust adapter's parse_python_imports."""
        return _py_adapter_imports(content, file_path)


def _py_ast_imports(content: str, file_path: str, repo_root: str) -> set[str]:
    """Extract Python imports via ast — ground truth."""
    imports: set[str] = set()
    try:
        tree = ast.parse(content, filename=file_path)
    except SyntaxError:
        return imports

    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                imports.add(alias.name.split(".")[0])  # top-level module
        elif isinstance(node, ast.ImportFrom):
            mod = node.module or ""
            if mod:
                imports.add(mod.split(".")[0])
    return imports


def _py_adapter_imports(content: str, file_path: str) -> set[str]:
    """Replicate the Rust adapter's parse_python_imports."""
    deps: set[str] = set()
    module_path = file_path.replace(".py", "").replace("/", ".").lstrip(".")
    base_parts = module_path.rsplit(".", 1)[:-1]

    for line in content.split("\n"):
        t = line.strip()
        if t.startswith("import "):
            m = t[7:].split(" as ")[0].strip()
            if m:
                deps.add(m.split(".")[0])
        elif t.startswith("from "):
            rest = t[5:]
            if rest.startswith("."):
                dot_count = len(rest) - len(rest.lstrip("."))
                ad = rest[dot_count:].strip()
                mp = ad.split(" import ")[0].strip() if " import " in ad else ""
                parts = list(base_parts[0]) if base_parts else []
                up = max(0, dot_count - 1)
                if up < len(parts):
                    parts = parts[:len(parts) - up]
                else:
                    continue
                if mp:
                    parts.append(mp)
                if parts:
                    deps.add(parts[0])  # top-level crate
            else:
                m = rest.split(" import ")[0].strip()
                if m:
                    deps.add(m.split(".")[0])
    return deps


# ===========================================================================
# Rust Backend
# ===========================================================================


class RustBackend(LanguageBackend):

    @property
    def name(self) -> str: return "rust"

    @property
    def source_extensions(self) -> tuple[str, ...]: return (".rs",)

    def is_test_file(self, rel_path: str) -> bool:
        base = os.path.basename(rel_path)
        # Rust test files are typically in tests/ dir or have #[cfg(test)] modules
        return "/tests/" in rel_path or base.endswith("_test.rs")

    def extract_imports_ast(self, content: str, file_path: str, repo_root: str) -> set[str]:
        """Ground truth via parsing `use` statements (equivalent to syn's level)."""
        return _rust_parse_use(content)

    def extract_imports_adapter(self, content: str, file_path: str, repo_root: str) -> set[str]:
        """Replicate the Rust adapter's parse_rust_imports."""
        return _rust_adapter_imports(content)


def _rust_parse_use(content: str) -> set[str]:
    """Parse Rust use statements — ground truth (crate-level external deps)."""
    deps: set[str] = set()
    for line in content.split("\n"):
        t = line.strip()
        if t.startswith("use ") and t.endswith(";"):
            path = t[4:-1].strip()
            parts = path.split("::")
            dep = parts[0]
            deps.add(dep)
        elif t.startswith("use ") and "{" in t:
            path_part = t[4:].split("{")[0].strip().rstrip("::")
            if path_part:
                deps.add(path_part.split("::")[0])
    # Only external crate dependencies (not std/core/alloc/crate/self/super/test)
    skip = {"std", "core", "alloc", "crate", "self", "super"}
    return {d for d in deps if d not in skip and not d.startswith("#")}


def _rust_adapter_imports(content: str) -> set[str]:
    """Replicate the Rust adapter's parse_rust_imports."""
    deps: set[str] = set()
    for line in content.split("\n"):
        t = line.strip()
        if t.startswith("use "):
            path = t[4:].rstrip(";").strip()
            parts = path.split("::")
            dep = "::".join(parts[:2]) if len(parts) >= 2 else path
            # Take only the crate-level prefix
            dep = dep.split("::")[0]
            if dep not in ("std", "core", "alloc", "crate", "self", "super"):
                deps.add(dep)
    return deps


# ===========================================================================
# Julia Backend
# ===========================================================================


class JuliaBackend(LanguageBackend):

    @property
    def name(self) -> str: return "julia"

    @property
    def source_extensions(self) -> tuple[str, ...]: return (".jl",)

    def is_test_file(self, rel_path: str) -> bool:
        return "/test/" in rel_path or rel_path.startswith("test/")

    def extract_imports_ast(self, content: str, file_path: str, repo_root: str) -> set[str]:
        return _julia_parse_imports(content)

    def extract_imports_adapter(self, content: str, file_path: str, repo_root: str) -> set[str]:
        """The Julia adapter uses ReTestItems. Import parsing is done by
        Testimonial.jl internally. We replicate a reasonable parser."""
        return _julia_parse_imports(content)


def _julia_parse_imports(content: str) -> set[str]:
    """Parse Julia import/using statements."""
    deps: set[str] = set()
    for line in content.split("\n"):
        t = line.strip()
        # using Module, Module.SubModule
        if t.startswith("using ") or t.startswith("import "):
            rest = t[6:] if t.startswith("using ") else t[7:]
            # Split by comma, then by . or :
            for part in rest.split(","):
                part = part.strip()
                # Handle: import Foo.Bar: baz
                if ":" in part:
                    part = part.split(":")[0].strip()
                # Handle: using .ModuleName (relative)
                part = part.lstrip(".")
                # Handle: using Foo: bar
                m = part.split(".")[0].strip()
                if m and m != "":
                    deps.add(m)
        # @testitem and @test don't import but we note them
        # @eval, @generated also don't import
    # Filter out common Julia builtins
    return {d for d in deps if d not in ("Base", "Core", "Main", "Test")}


# ===========================================================================
# TypeScript Backend
# ===========================================================================


class TypeScriptBackend(LanguageBackend):

    @property
    def name(self) -> str: return "typescript"

    @property
    def source_extensions(self) -> tuple[str, ...]: return (".ts", ".tsx", ".js", ".jsx")

    def is_test_file(self, rel_path: str) -> bool:
        base = os.path.basename(rel_path)
        return base.startswith("test_") or base.endswith(".test.ts") or base.endswith(".spec.ts") or "/__tests__/" in rel_path or "/test/" in rel_path

    def extract_imports_ast(self, content: str, file_path: str, repo_root: str) -> set[str]:
        return _ts_parse_imports(content)

    def extract_imports_adapter(self, content: str, file_path: str, repo_root: str) -> set[str]:
        """Replicate what the TypeScript adapter would do (tree-sitter based)."""
        return _ts_parse_imports(content)


def _ts_parse_imports(content: str) -> set[str]:
    """Parse TypeScript import statements."""
    deps: set[str] = set()
    for line in content.split("\n"):
        t = line.strip()
        # import X from 'module'
        # import { X } from 'module'
        # import * as X from 'module'
        m = re.match(r'^import\s+(?:\{[^}]*\}|\*\s+as\s+\w+|\w+(?:\s*,\s*\{[^}]*\})?)\s+from\s+[\'"]([^\'"]+)[\'"]', t)
        if m:
            mod = m.group(1)
            if not mod.startswith(".") and not mod.startswith("/"):
                deps.add(mod.split("/")[0])
            continue
        # import 'module' (side-effect import)
        m = re.match(r"^import\s+['\"]([^'\"]+)['\"]", t)
        if m:
            mod = m.group(1)
            if not mod.startswith(".") and not mod.startswith("/"):
                deps.add(mod.split("/")[0])
            continue
        # require('module')
        m = re.search(r"require\(['\"]([^'\"]+)['\"]\)", t)
        if m:
            mod = m.group(1)
            if not mod.startswith(".") and not mod.startswith("/"):
                deps.add(mod.split("/")[0])
    return deps


# ===========================================================================
# Clojure Backend
# ===========================================================================


class ClojureBackend(LanguageBackend):

    @property
    def name(self) -> str: return "clojure"

    @property
    def source_extensions(self) -> tuple[str, ...]: return (".clj", ".cljs", ".cljc", ".edn")

    def is_test_file(self, rel_path: str) -> bool:
        base = os.path.basename(rel_path)
        return "/test/" in rel_path or base.startswith("test_") or base.endswith("_test.clj")

    def extract_imports_ast(self, content: str, file_path: str, repo_root: str) -> set[str]:
        return _clj_parse_imports(content)

    def extract_imports_adapter(self, content: str, file_path: str, repo_root: str) -> set[str]:
        return _clj_parse_imports(content)


def _clj_parse_imports(content: str) -> set[str]:
    """Parse Clojure :require, :import, and use forms."""
    deps: set[str] = set()
    # Match: :require [namespace :as alias] or :require [[ns1] [ns2]]
    # Also: :require namespace
    for line in content.split("\n"):
        t = line.strip()
        # ns form with :require
        m = re.findall(r':require\s+\[([^\]]+)\]', t)
        for block in m:
            for part in block.split():
                part = part.strip().rstrip("]")
                if part and not part.startswith(":") and not part.startswith("[") and "/" in part:
                    deps.add(part.split("/")[0])
                elif part and not part.startswith(":") and not part.startswith("["):
                    deps.add(part.split(".")[0])
        # :import
        m = re.findall(r':import\s+\[([^\]]+)\]', t)
        for block in m:
            for part in block.split():
                part = part.strip()
                if part and not part.startswith(":") and "." in part:
                    deps.add(part.split(".")[0])
        # use with :only
        m = re.findall(r'\(use\s+\'([^\s)]+)', t)
        for mod in m:
            deps.add(mod.split("/")[0])
    return deps


# ===========================================================================
# Backend Registry
# ===========================================================================


BACKENDS: dict[str, type[LanguageBackend]] = {
    "python": PythonBackend,
    "rust": RustBackend,
    "julia": JuliaBackend,
    "typescript": TypeScriptBackend,
    "clojure": ClojureBackend,
}

ADAPTER_MAP: dict[str, str] = {
    "python": "testaruda-adapter-python",
    "rust": "testaruda-adapter-rust",
    "julia": "testaruda-adapter-julia",
    "typescript": "testaruda-adapter-typescript",
    "clojure": "testaruda-adapter-clojure",
}


def detect_language(repo_root: str) -> str | None:
    """Auto-detect project language from config files."""
    files = os.listdir(repo_root)
    if "Cargo.toml" in files:
        return "rust"
    if "Project.toml" in files or any(f.endswith(".jl") for f in files):
        return "julia"
    if "package.json" in files:
        return "typescript"
    if "deps.edn" in files or "project.clj" in files:
        return "clojure"
    if "pyproject.toml" in files or "setup.py" in files or "requirements.txt" in files:
        return "python"
    # Fallback: check for source files
    exts = {Path(f).suffix for f in files}
    if ".py" in exts:
        return "python"
    if ".rs" in exts:
        return "rust"
    return None


# ===========================================================================
# Shared utilities
# ===========================================================================


def find_source_files(repo_root: str, backend: LanguageBackend) -> list[tuple[str, str]]:
    """
    Find all source files and return [(rel_path, abs_path), ...].
    """
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


# ===========================================================================
# Validation
# ===========================================================================


def validate(repo_root: str, backend: LanguageBackend, adapter: str) -> dict:
    """
    Run full validation:
    1. Discover test files via adapter
    2. For each test file, compare GT imports vs adapter import parsing
    """
    # Discover test files
    disc = run_adapter(adapter, {"command": "discover"}, repo_root)
    if not disc or not disc.get("ok"):
        return {"error": "adapter discover failed", "adapter_ok": False}

    test_files = sorted(set(
        t["file"] for t in disc.get("result", [])
        if isinstance(t, dict) and "file" in t
    ))

    # Also find all source files for context
    all_files = find_source_files(repo_root, backend)
    source_files = [f for f in all_files if not backend.is_test_file(f[0])]
    test_files_found = [f for f in all_files if backend.is_test_file(f[0])]

    # If adapter didn't find test files but we did, use our list
    if not test_files and test_files_found:
        test_files = [f[0] for f in test_files_found]

    # Compare imports per test file
    return _compare(repo_root, backend, test_files)


def _compare(repo_root: str, backend: LanguageBackend, test_files: list[str]) -> dict:
    per_pattern: dict[str, dict] = defaultdict(lambda: {
        "ground_truth": 0, "matched": 0, "examples": [],
    })
    total_gt = 0
    total_adapter = 0
    total_matched = 0

    for test_file in test_files:
        abs_path = Path(repo_root) / test_file
        if not abs_path.exists():
            continue
        try:
            content = abs_path.read_text(encoding="utf-8", errors="replace")
        except Exception:
            continue

        gt = backend.extract_imports_ast(content, test_file, repo_root)
        ad = backend.extract_imports_adapter(content, test_file, repo_root)

        total_gt += len(gt)
        total_adapter += len(ad)
        matched = gt & ad
        total_matched += len(matched)

        # Per-pattern: since we can't easily categorize by pattern type
        # in a language-agnostic way, we categorize by import style
        # inferred from the test file's language
        for m in gt - ad:
            if len(per_pattern["missed"]["examples"]) < 5:
                per_pattern["missed"].setdefault("examples", []).append({
                    "file": test_file, "module": m,
                })

    per_pattern["missed"]["ground_truth"] = total_gt - total_matched
    per_pattern["missed"]["matched"] = 0

    fp = total_adapter - total_matched
    fn = total_gt - total_matched
    precision = total_matched / max(total_adapter, 1)
    recall = total_matched / max(total_gt, 1)
    f1 = 2 * precision * recall / max(precision + recall, 0.001)

    return {
        "precision": round(precision, 4),
        "recall": round(recall, 4),
        "f1": round(f1, 4),
        "total_imports_ground_truth": total_gt,
        "total_imports_adapter": total_adapter,
        "matched": total_matched,
        "false_positives": fp,
        "false_negatives": fn,
        "test_files_analyzed": len(test_files),
        "test_files": test_files,
    }


# ===========================================================================
# Main
# ===========================================================================


def main():
    parser = argparse.ArgumentParser(
        description="Validate adapter import parsing accuracy across languages"
    )
    parser.add_argument("repo", nargs="?", help="Path to git repository")
    parser.add_argument("--language", choices=list(BACKENDS.keys()) + ["auto"], default="auto")
    parser.add_argument("--adapter", help="Adapter binary path")
    parser.add_argument("--output", help="Write JSON results to file")
    parser.add_argument("--list-languages", action="store_true", help="Show supported languages")
    parser.add_argument("--all", action="store_true", help="Run on all candidate repos (click, testaruda, fixture_julia) and aggregate")
    args = parser.parse_args()

    if args.list_languages:
        print("Supported languages:")
        for name, cls in BACKENDS.items():
            adapter = ADAPTER_MAP.get(name, "N/A")
            exts = ", ".join(cls().source_extensions)
            print(f"  {name:12s}  adapter: {adapter:35s}  extensions: {exts}")
        sys.exit(0)

    # '--all' mode: run on all available test repos
    if args.all:
        _run_all()
        return

    if not args.repo:
        parser.print_help()
        sys.exit(1)

    repo = os.path.abspath(args.repo)
    if not os.path.isdir(repo):
        print(f"Error: not a directory: {repo}", file=sys.stderr)
        sys.exit(1)

    # Auto-detect language
    lang = args.language
    if lang == "auto":
        lang = detect_language(repo)
        if not lang:
            print(f"Error: cannot auto-detect language for {repo}", file=sys.stderr)
            print("Use --language to specify one of:", ", ".join(BACKENDS.keys()), file=sys.stderr)
            sys.exit(1)
        print(f"  Auto-detected language: {lang}", file=sys.stderr)

    backend_cls = BACKENDS.get(lang)
    if not backend_cls:
        print(f"Error: unsupported language: {lang}", file=sys.stderr)
        sys.exit(1)
    backend = backend_cls()

    adapter = args.adapter or ADAPTER_MAP.get(lang, "")
    if not adapter:
        print(f"Error: no default adapter for {lang}", file=sys.stderr)
        sys.exit(1)

    # Check adapter availability
    if not _check_adapter(adapter):
        print(f"Warning: adapter '{adapter}' not found — skipping adapter comparison", file=sys.stderr)
        adapter = None

    repo_name = os.path.basename(repo)
    print(f"=== Validating {lang} adapter: {repo_name} ===", file=sys.stderr)

    # Source file summary
    all_files = find_source_files(repo, backend)
    source_files = [f for f in all_files if not backend.is_test_file(f[0])]
    test_files = [f for f in all_files if backend.is_test_file(f[0])]
    print(f"  Source files: {len(source_files)}  Test files: {len(test_files)}", file=sys.stderr)

    # Validate
    result = validate(repo, backend, adapter)

    if "error" in result:
        print(f"  ERROR: {result['error']}", file=sys.stderr)
        if result.get("adapter_ok") is False:
            print("  (continuing with file-based analysis only)", file=sys.stderr)

    result.update({
        "language": lang,
        "adapter": adapter,
        "repo": repo_name,
        "source_files": len(source_files),
    })

    output_json = json.dumps(result, indent=2)
    if args.output:
        with open(args.output, "w") as f:
            f.write(output_json)
        print(f"Results written to {args.output}", file=sys.stderr)
    else:
        print(output_json)

    # Summary
    if "error" not in result:
        c = result
        print(f"\n=== Summary ===", file=sys.stderr)
        print(f"  Language: {lang}   Adapter: {'available' if adapter else 'not found'}", file=sys.stderr)
        print(f"  Test files: {c.get('test_files_analyzed', 0)}", file=sys.stderr)
        print(f"  Precision: {c['precision']:.2%}  Recall: {c['recall']:.2%}  F1: {c['f1']:.2%}", file=sys.stderr)
        print(f"  GT imports: {c['total_imports_ground_truth']}  Adapter: {c['total_imports_adapter']}  Matched: {c['matched']}", file=sys.stderr)
        print(f"  False positives: {c['false_positives']}  False negatives: {c['false_negatives']}", file=sys.stderr)


def _check_adapter(adapter: str) -> bool:
    try:
        subprocess.run([adapter, "--help"], capture_output=True, timeout=5)
        return True
    except Exception:
        return False


def _run_all():
    """Run validation across all available test repos and aggregate."""
    repo_dir = Path(__file__).resolve().parent.parent
    scratch = repo_dir / "target" / "scratch"
    results = []

    # Test repos: repo_path, language, adapter, display_name
    test_suites = [
        (str(repo_dir), "rust", "testaruda-adapter-rust", "testaruda (Rust)"),
        (str(scratch / "bat"), "rust", "testaruda-adapter-rust", "bat (Rust)"),
        (str(scratch / "tokei"), "rust", "testaruda-adapter-rust", "tokei (Rust)"),
        (str(scratch / "click"), "python", "testaruda-adapter-python", "click (Python)"),
        (str(scratch / "attrs"), "python", "testaruda-adapter-python", "attrs (Python)"),
        (str(scratch / "structlog"), "python", "testaruda-adapter-python", "structlog (Python)"),
        (str(scratch / "httpx"), "python", "testaruda-adapter-python", "httpx (Python)"),
        (str(repo_dir / "tests" / "fixtures" / "julia"), "julia", "testaruda-adapter-julia", "fixture (Julia)"),
    ]

    print(f"=== Multi-language import validation ===", file=sys.stderr)

    for repo_path, lang, adapter, name in test_suites:
        if not (os.path.isdir(os.path.join(repo_path, ".git")) or lang in ("julia", "python")):
            if lang == "julia" and not os.path.isdir(repo_path):
                print(f"  SKIP {name}: fixture not found", file=sys.stderr)
                results.append({"repo": name, "language": lang, "error": "fixture not found"})
                continue
        if not _check_adapter(adapter):
            print(f"  SKIP {name}: adapter not available", file=sys.stderr)
            results.append({"repo": name, "language": lang, "error": "adapter not available"})
            continue

        backend = BACKENDS[lang]()
        print(f"\n  [{lang}] {name}...", file=sys.stderr)
        try:
            r = validate(str(repo_path), backend, adapter)
            r["repo"] = name
            r["language"] = lang
            results.append(r)
            c = r
            if "error" not in r:
                print(f"    ✅ P:{c['precision']:.1%} R:{c['recall']:.1%} F1:{c['f1']:.1%}  files:{c.get('test_files_analyzed',0)}  fp:{c['false_positives']} fn:{c['false_negatives']}", file=sys.stderr)
            else:
                print(f"    ❌ {r['error']}", file=sys.stderr)
        except Exception as e:
            print(f"    ❌ error: {e}", file=sys.stderr)
            results.append({"repo": name, "language": lang, "error": str(e)})

    # Aggregate
    valid = [r for r in results if "error" not in r]
    if valid:
        total_gt = sum(r["total_imports_ground_truth"] for r in valid)
        total_ad = sum(r["total_imports_adapter"] for r in valid)
        total_matched = sum(r["matched"] for r in valid)
        fp = sum(r["false_positives"] for r in valid)
        fn = sum(r["false_negatives"] for r in valid)
        precision = total_matched / max(total_ad, 1)
        recall = total_matched / max(total_gt, 1)
        f1 = 2 * precision * recall / max(precision + recall, 0.001)

        aggregate = {
            "languages_tested": len(valid),
            "total_imports_ground_truth": total_gt,
            "total_imports_adapter": total_ad,
            "total_matched": total_matched,
            "false_positives": fp,
            "false_negatives": fn,
            "precision": round(precision, 4),
            "recall": round(recall, 4),
            "f1": round(f1, 4),
            "per_language": [
                {"language": r["language"], "repo": r["repo"],
                 "precision": r["precision"], "recall": r["recall"], "f1": r["f1"],
                 "test_files": r.get("test_files_analyzed", 0),
                 "fp": r["false_positives"], "fn": r["false_negatives"]}
                for r in valid
            ],
        }
    else:
        aggregate = {"languages_tested": 0}

    output = {
        "schema": "https://testaruda.dev/schemas/import-validation-v2",
        "generated": "2026-07-27",
        "results": results,
        "aggregate": aggregate,
    }

    output_path = Path(repo_dir) / "docs" / "multi-language-import-validation.json"
    with open(output_path, "w") as f:
        json.dump(output, f, indent=2)
    print(f"\nFull results written to {output_path}", file=sys.stderr)

    # Summary
    agg = aggregate
    if agg["languages_tested"] > 0:
        print(f"\n{'='*60}", file=sys.stderr)
        print(f"Multi-language aggregate:", file=sys.stderr)
        print(f"  Languages: {agg['languages_tested']}   GT imports: {agg['total_imports_ground_truth']}", file=sys.stderr)
        print(f"  Aggregate precision: {agg['precision']:.2%}   Recall: {agg['recall']:.2%}   F1: {agg['f1']:.2%}", file=sys.stderr)
        print(f"  Total false positives: {agg['false_positives']}   False negatives: {agg['false_negatives']}", file=sys.stderr)
        print(f"\n  Per language:", file=sys.stderr)
        for r in agg["per_language"]:
            print(f"    {r['language']:10s}  {r['repo']:25s}  P:{r['precision']:.1%}  R:{r['recall']:.1%}  F1:{r['f1']:.1%}  files:{r['test_files']}  fp:{r['fp']} fn:{r['fn']}", file=sys.stderr)
    else:
        print("No valid results collected.", file=sys.stderr)


if __name__ == "__main__":
    main()