"""Validation: compare ground-truth imports against adapter discovery."""

from collections import defaultdict
from pathlib import Path

from validate_imports.base import LanguageBackend
from validate_imports.discovery import find_source_files, run_adapter


def _adapter_discovered_test_files(
    backend: LanguageBackend, adapter: str, repo_root: str
) -> list[str]:
    """Test files as reported by the adapter's discover (Julia discovers
    globally, not per-repo, so it never uses this path)."""
    if backend.name == "julia":
        return []
    disc = run_adapter(adapter, {"command": "discover"}, repo_root)
    if not (disc and disc.get("ok")):
        return []
    return sorted(set(
        t["file"] for t in disc.get("result", [])
        if isinstance(t, dict) and "file" in t
    ))


def _testitem_test_files(all_files: list[tuple[str, str]]) -> list[str]:
    """Julia ground truth: files with @testitem blocks under /test/."""
    testitem_files = []
    for rel_path, abs_path in all_files:
        try:
            content = Path(abs_path).read_text(encoding="utf-8", errors="replace")
            if "@testitem" in content and "/test/" in rel_path:
                testitem_files.append(rel_path)
        except Exception:
            continue
    return sorted(set(testitem_files))


def _resolve_test_files(
    backend: LanguageBackend,
    adapter: str,
    repo_root: str,
    all_files: list[tuple[str, str]],
) -> list[str]:
    test_files = _adapter_discovered_test_files(backend, adapter, repo_root)

    # For Julia: scan files for @testitem blocks as ground truth
    if backend.name == "julia" or not test_files:
        test_files = _testitem_test_files(all_files)

    # If still no test files, fall back to directory-based detection
    if not test_files:
        found = [f[0] for f in all_files if backend.is_test_file(f[0])]
        if found:
            test_files = found
    return test_files


def validate(repo_root: str, backend: LanguageBackend, adapter: str) -> dict:
    """Run full validation:
    1. Find test files (via adapter discover or file scan)
    2. For each test file, compare GT imports vs adapter import parsing
    """
    all_files = find_source_files(repo_root, backend)
    test_files = _resolve_test_files(backend, adapter, repo_root, all_files)
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

    return _metrics(per_pattern, total_gt, total_adapter, total_matched, test_files)


def _metrics(
    per_pattern: dict[str, dict],
    total_gt: int,
    total_adapter: int,
    total_matched: int,
    test_files: list[str],
) -> dict:
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
