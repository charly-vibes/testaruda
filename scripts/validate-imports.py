#!/usr/bin/env python3
"""
validate-imports.py — Validate Python adapter import parsing accuracy.

Compares the adapter's import detection (used by static-deps) against
a ground-truth import graph extracted via Python's ast module.
Reports precision, recall, and categorizes missed edges by import pattern.

Usage:
  ./scripts/validate-imports.py [options] <repo-path>

Options:
  --base REF       Git base ref (default: HEAD~5)
  --head REF       Git head ref (default: HEAD)
  --adapter PATH   Adapter binary path (default: testaruda-adapter-python)
  --output FILE    Write JSON results (default: stdout)
  --help           Show this help
"""

import argparse
import ast
import json
import os
import subprocess
import sys
from collections import defaultdict
from pathlib import Path


# ===========================================================================
# Ground-truth extraction (via Python ast module)
# ===========================================================================


class ImportCollector(ast.NodeVisitor):
    """Collects all import statements from a Python AST, categorized by pattern."""

    def __init__(self, file_path: str, repo_root: str):
        self.file_path = file_path
        self.repo_root = repo_root
        self.imports: list[dict] = []

    def _resolve_relative(self, dot_count: int, module_part: str | None) -> str | None:
        rel_path = Path(self.file_path).relative_to(self.repo_root)
        parts = list(rel_path.parent.parts)
        stem = rel_path.stem
        if stem != "__init__":
            parts.append(stem)

        if dot_count > 0:
            up = dot_count - 1
            if up >= len(parts):
                return None
            parts = parts[:-up] if up > 0 else parts

        if module_part:
            parts.append(module_part)
        return ".".join(parts) if parts else None

    def _is_type_checking(self, node) -> bool:
        parent = getattr(node, 'parent', None)
        while parent:
            if isinstance(parent, ast.If):
                guard = parent.test
                if isinstance(guard, ast.Name) and guard.id == "TYPE_CHECKING":
                    return True
                if (isinstance(guard, ast.Attribute) and
                    hasattr(guard.value, 'id') and guard.value.id == "typing" and
                    guard.attr == "TYPE_CHECKING"):
                    return True
            parent = getattr(parent, 'parent', None)
        return False

    def _in_function_body(self, node) -> bool:
        parent = getattr(node, 'parent', None)
        while parent:
            if isinstance(parent, (ast.FunctionDef, ast.AsyncFunctionDef)):
                return True
            parent = getattr(parent, 'parent', None)
        return False

    def _record(self, module: str, pattern: str, lineno: int,
                in_type_checking: bool, in_function: bool):
        self.imports.append({
            "module": module,
            "pattern": pattern,
            "lineno": lineno,
            "file": self.file_path,
            "in_type_checking": in_type_checking,
            "in_function": in_function,
        })

    def _walk_body(self, body, parent_node=None):
        for node in body:
            node.parent = parent_node or getattr(body, 'parent', None)

            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                for stmt in node.body:
                    stmt.parent = node
                    self._walk_body([stmt], node)
                for stmt in getattr(node, 'orelse', []):
                    stmt.parent = node
                    self._walk_body([stmt], node)
            elif isinstance(node, (ast.If, ast.Try, ast.With, ast.For, ast.While)):
                for stmt in node.body:
                    stmt.parent = node
                    self._walk_body([stmt], node)
                for stmt in getattr(node, 'orelse', []):
                    stmt.parent = node
                    self._walk_body([stmt], node)
                for handler in getattr(node, 'handlers', []):
                    for stmt in handler.body:
                        stmt.parent = handler
                        self._walk_body([stmt], handler)
                for stmt in getattr(node, 'finalbody', []):
                    stmt.parent = node
                    self._walk_body([stmt], node)
            elif isinstance(node, ast.Import):
                in_tc = self._is_type_checking(node)
                in_fn = self._in_function_body(node)
                for alias in node.names:
                    self._record(alias.name, "import", node.lineno, in_tc, in_fn)
            elif isinstance(node, ast.ImportFrom):
                in_tc = self._is_type_checking(node)
                in_fn = self._in_function_body(node)
                level = node.level or 0
                module = node.module or ""
                if level > 0:
                    resolved = self._resolve_relative(level, module)
                    if resolved:
                        self._record(resolved, f"from_relative({level})", node.lineno, in_tc, in_fn)
                else:
                    self._record(module, "from_absolute", node.lineno, in_tc, in_fn)

    def visit_Module(self, node: ast.Module):
        self._walk_body(node.body)


def extract_source_files(repo_root: str) -> list[str]:
    excludes = {
        ".venv", "venv", "__pycache__", ".mypy_cache", ".pytest_cache",
        "build", "dist", ".git", "target", "node_modules", ".tox",
        ".eggs", "*.egg-info", ".direnv",
    }
    sources = []
    repo = Path(repo_root).resolve()
    for f in repo.rglob("*.py"):
        rel = f.relative_to(repo)
        parts = rel.parts
        if any(p in excludes for p in parts):
            continue
        # Skip venv markers
        if any("site-packages" in p for p in parts):
            continue
        sources.append(str(rel))
    return sorted(sources)


def extract_ground_truth(repo_root: str) -> dict:
    """
    Extract the complete import graph via ast.
    Returns a dict of rel_path → list of imported module names.
    """
    source_files = extract_source_files(repo_root)
    gt: dict[str, set[str]] = defaultdict(set)

    for rel_path in source_files:
        abs_path = Path(repo_root) / rel_path
        try:
            content = abs_path.read_text(encoding="utf-8", errors="replace")
        except Exception:
            continue
        try:
            tree = ast.parse(content, filename=str(abs_path))
        except SyntaxError:
            continue

        collector = ImportCollector(str(abs_path), repo_root)
        collector.visit(tree)
        for imp in collector.imports:
            gt[rel_path].add(imp["module"])

    return {
        "edges": {k: sorted(v) for k, v in gt.items()},
        "source_count": len(source_files),
    }


# ===========================================================================
# Adapter import extraction
# ===========================================================================


def send_adapter_command(adapter: str, cmd: dict, cwd: str) -> dict | None:
    cmd_json = json.dumps(cmd)
    try:
        result = subprocess.run(
            [adapter], input=cmd_json, capture_output=True,
            text=True, timeout=30, cwd=cwd,
        )
        if result.returncode != 0:
            return None
        first = result.stdout.strip().split("\n")[0]
        return json.loads(first)
    except (subprocess.TimeoutExpired, json.JSONDecodeError, FileNotFoundError):
        return None


def get_adapter_imports(repo_root: str, adapter: str, test_file: str) -> set[str]:
    """
    Get the adapter's detected imports for a test file by running
    discover and then static-deps with the test file as the changed file.
    The adapter parses the file's imports and returns edges.
    """
    # Run discover first to get test node_id mapping
    disc = send_adapter_command(adapter, {"command": "discover"}, repo_root)
    if not disc or not disc.get("ok"):
        return set()

    # Run static-deps with just this file as changed
    sd = send_adapter_command(adapter, {
        "command": "static-deps",
        "params": {"changed_files": [test_file]},
    }, repo_root)
    if not sd or not sd.get("ok"):
        return set()

    # Collect imported modules from edges
    imports: set[str] = set()
    for edge in sd.get("edges", []):
        if isinstance(edge, dict):
            # The "from" is the test node, "to" is the source file it imports
            to_file = edge.get("to", "")
            if to_file:
                imports.add(to_file)
    return imports


# ===========================================================================
# Adapter regex-based import parser (replicating the Rust logic in Python)
# ===========================================================================


def adapter_parse_imports(content: str, file_path: str) -> set[str]:
    """
    Replicate the Rust adapter's parse_python_imports logic in Python
    for a more direct comparison (without subprocess overhead on every file).
    
    This avoids running the adapter N times for N files.
    """
    # file_path_to_module equivalent
    module_path = file_path.replace(".py", "").replace("/", ".").lstrip(".")
    base_package_parts = module_path.rsplit(".", 1)[:-1]  # skip the module name

    deps: set[str] = set()

    for line in content.split("\n"):
        trimmed = line.strip()

        if trimmed.startswith("import "):
            module = trimmed[7:].split(" as ")[0].strip()
            if module:
                deps.add(module)

        elif trimmed.startswith("from "):
            rest = trimmed[5:]
            if rest.startswith("."):
                # Relative import
                dot_count = 0
                while dot_count < len(rest) and rest[dot_count] == ".":
                    dot_count += 1
                after_dots = rest[dot_count:].strip()

                if after_dots.startswith("import"):
                    module_part = ""
                else:
                    module_part = after_dots.split(" import ")[0].strip() if " import " in after_dots else ""

                parts = list(base_package_parts[0]) if base_package_parts else []
                levels_up = max(0, dot_count - 1)
                if levels_up < len(parts):
                    parts = parts[:len(parts) - levels_up]
                else:
                    continue  # above root

                if module_part:
                    parts.append(module_part)

                if parts:
                    deps.add(".".join(parts))
            else:
                module = rest.split(" import ")[0].strip() if " import " in rest else ""
                if module:
                    deps.add(module)

    return deps


# ===========================================================================
# Comparison
# ===========================================================================


def classify_import_pattern(imp: dict) -> str:
    if imp.get("in_type_checking"):
        return "type_checking"
    pat = imp["pattern"]
    if pat.startswith("from_relative"):
        levels = pat.split("(")[1].rstrip(")")
        if levels == "1":
            return "relative_current"
        elif levels == "2":
            return "relative_parent"
        else:
            return "relative_deep"
    if pat == "from_absolute":
        return "from_absolute"
    if pat == "import":
        if imp.get("in_function"):
            return "lazy_import"
        return "import_plain"
    return "other"


def compare(repo_root: str, test_files: list[str]) -> dict:
    """
    For each test file, compare the adapter's import detection vs AST ground truth.
    Returns precision, recall, and per-pattern breakdown.
    """
    per_pattern: dict[str, dict] = defaultdict(lambda: {
        "ground_truth": 0, "adapter_found": 0, "matched": 0, "examples": [],
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

        # GT imports via AST
        try:
            tree = ast.parse(content, filename=str(abs_path))
        except SyntaxError:
            continue
        collector = ImportCollector(str(abs_path), repo_root)
        collector.visit(tree)

        gt_modules: set[str] = set()
        for imp in collector.imports:
            gt_modules.add(imp["module"])

        # Adapter imports via replicated logic
        adapter_modules = adapter_parse_imports(content, test_file)

        total_gt += len(gt_modules)
        total_adapter += len(adapter_modules)

        matched = gt_modules & adapter_modules
        total_matched += len(matched)

        # Per-pattern breakdown
        for imp in collector.imports:
            pattern = classify_import_pattern(imp)
            per_pattern[pattern]["ground_truth"] += 1
            if imp["module"] in adapter_modules:
                per_pattern[pattern]["matched"] += 1
            else:
                per_pattern[pattern]["adapter_found"] = per_pattern[pattern].get("adapter_found", 0)
                if len(per_pattern[pattern]["examples"]) < 3:
                    per_pattern[pattern]["examples"].append({
                        "file": test_file,
                        "module": imp["module"],
                        "line": imp["lineno"],
                        "pattern_detail": imp["pattern"],
                    })

    # Also count what adapter found that GT didn't (false positives)
    false_positives = total_adapter - total_matched
    false_negatives = total_gt - total_matched

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
        "false_positives": false_positives,
        "false_negatives": false_negatives,
        "per_pattern": dict(per_pattern),
    }


# ===========================================================================
# Main
# ===========================================================================


def main():
    parser = argparse.ArgumentParser(
        description="Validate Python adapter import parsing accuracy"
    )
    parser.add_argument("repo", help="Path to git repository")
    parser.add_argument("--base", default="HEAD~5", help="Git base ref")
    parser.add_argument("--head", default="HEAD", help="Git head ref")
    parser.add_argument("--adapter", default="testaruda-adapter-python", help="Adapter binary")
    parser.add_argument("--output", help="Write JSON results to file")
    args = parser.parse_args()

    repo = os.path.abspath(args.repo)
    if not os.path.isdir(os.path.join(repo, ".git")):
        print(f"Error: not a git repository: {repo}", file=sys.stderr)
        sys.exit(1)

    repo_name = os.path.basename(repo)
    print(f"=== Validating import parsing accuracy: {repo_name} ===", file=sys.stderr)

    # Step 1: Ground truth
    print("  [1/4] Extracting ground-truth import graph (ast)...", file=sys.stderr)
    ground_truth = extract_ground_truth(repo)
    print(f"    Found {ground_truth['source_count']} source files", file=sys.stderr)

    # Step 2: Discover test files via the adapter
    print("  [2/4] Discovering test files via adapter...", file=sys.stderr)
    disc = send_adapter_command(args.adapter, {"command": "discover"}, repo)
    if not disc or not disc.get("ok"):
        print("    ERROR: adapter discover failed", file=sys.stderr)
        sys.exit(1)

    test_files = sorted(set(
        t["file"] for t in disc.get("result", []) if isinstance(t, dict) and "file" in t
    ))
    print(f"    Found {len(test_files)} test files", file=sys.stderr)

    # Step 3: Compare imports per test file
    print("  [3/4] Comparing imports per test file...", file=sys.stderr)
    comparison = compare(repo, test_files)

    # Step 4: Output
    output = {
        "schema": "https://testaruda.dev/schemas/import-validation-v1",
        "repo": repo_name,
        "adapter": args.adapter,
        "ground_truth": {
            "source_files": ground_truth["source_count"],
        },
        "comparison": comparison,
        "test_files": test_files,
    }

    output_json = json.dumps(output, indent=2)
    if args.output:
        with open(args.output, "w") as f:
            f.write(output_json)
        print(f"Results written to {args.output}", file=sys.stderr)
    else:
        print(output_json)

    c = comparison
    print(f"\n=== Summary ===", file=sys.stderr)
    print(f"  Test files analyzed: {len(test_files)}", file=sys.stderr)
    print(f"  Precision: {c['precision']:.2%}  Recall: {c['recall']:.2%}  F1: {c['f1']:.2%}", file=sys.stderr)
    print(f"  GT imports: {c['total_imports_ground_truth']}  Adapter found: {c['total_imports_adapter']}  Matched: {c['matched']}", file=sys.stderr)
    print(f"  False positives: {c['false_positives']}  False negatives: {c['false_negatives']}", file=sys.stderr)

    if c.get("per_pattern"):
        print(f"\n  Per-pattern breakdown:", file=sys.stderr)
        for pattern, data in sorted(c["per_pattern"].items()):
            missed = data["ground_truth"] - data["matched"]
            rate = missed / max(data["ground_truth"], 1)
            bar = "█" * int(rate * 20) + "░" * (20 - int(rate * 20))
            print(f"    {pattern:20s}  {bar}  {data['matched']}/{data['ground_truth']} matched", file=sys.stderr)
            for ex in data["examples"][:3]:
                print(f"      ✗ {ex['file']}:{ex['line']}  {ex['module']} ({ex['pattern_detail']})", file=sys.stderr)


if __name__ == "__main__":
    main()