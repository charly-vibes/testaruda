"""CLI entry points for the validate-imports shim."""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

from validate_imports import ADAPTER_MAP, BACKENDS
from validate_imports.discovery import detect_language, find_source_files
from validate_imports.validation import validate

# Test repos: repo_path, language, adapter, display_name
def _test_suites() -> list[tuple[str, str, str, str]]:
    repo_dir = Path(__file__).resolve().parent.parent.parent
    scratch = repo_dir / "target" / "scratch"
    suites = [
        (str(repo_dir), "rust", "testaruda-adapter-rust", "testaruda (Rust)"),
        (str(scratch / "bat"), "rust", "testaruda-adapter-rust", "bat (Rust)"),
        (str(scratch / "tokei"), "rust", "testaruda-adapter-rust", "tokei (Rust)"),
        (str(scratch / "click"), "python", "testaruda-adapter-python", "click (Python)"),
        (str(scratch / "attrs"), "python", "testaruda-adapter-python", "attrs (Python)"),
        (str(scratch / "structlog"), "python", "testaruda-adapter-python", "structlog (Python)"),
        (str(scratch / "httpx"), "python", "testaruda-adapter-python", "httpx (Python)"),
        (str(repo_dir / "tests" / "fixtures" / "julia"), "julia", "testaruda-adapter-julia", "fixture (Julia)"),
        (str(scratch / "json-jl"), "julia", "testaruda-adapter-julia", "JSON.jl"),
        (str(scratch / "csv-jl"), "julia", "testaruda-adapter-julia", "CSV.jl"),
        (str(scratch / "http-jl"), "julia", "testaruda-adapter-julia", "HTTP.jl"),
        (str(scratch / "dataframes-jl"), "julia", "testaruda-adapter-julia", "DataFrames.jl"),
        (str(scratch / "plots-jl"), "julia", "testaruda-adapter-julia", "Plots.jl"),
    ]
    # Add Testimonial.jl if available
    testimonial = Path(os.path.expanduser("~")) / "para" / "areas" / "dev" / "gh" / "sk" / "Testimonial.jl"
    if testimonial.exists():
        suites.append((str(testimonial), "julia", "testaruda-adapter-julia", "Testimonial.jl"))
    return suites


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Validate adapter import parsing accuracy across languages"
    )
    parser.add_argument("repo", nargs="?", help="Path to git repository")
    parser.add_argument("--language", choices=list(BACKENDS.keys()) + ["auto"], default="auto")
    parser.add_argument("--adapter", help="Adapter binary path")
    parser.add_argument("--output", help="Write JSON results to file")
    parser.add_argument("--list-languages", action="store_true", help="Show supported languages")
    parser.add_argument("--all", action="store_true", help="Run on all candidate repos (click, testaruda, fixture_julia) and aggregate")
    return parser


def _print_language_list() -> None:
    print("Supported languages:")
    for name, cls in BACKENDS.items():
        adapter = ADAPTER_MAP.get(name, "N/A")
        exts = ", ".join(cls().source_extensions)
        print(f"  {name:12s}  adapter: {adapter:35s}  extensions: {exts}")


def _resolve_language(args: argparse.Namespace) -> str:
    lang = args.language
    if lang != "auto":
        return lang
    repo = os.path.abspath(args.repo)
    lang = detect_language(repo)
    if not lang:
        print(f"Error: cannot auto-detect language for {repo}", file=sys.stderr)
        print("Use --language to specify one of:", ", ".join(BACKENDS.keys()), file=sys.stderr)
        sys.exit(1)
    print(f"  Auto-detected language: {lang}", file=sys.stderr)
    return lang


def _run_single(args: argparse.Namespace) -> None:
    repo = os.path.abspath(args.repo)
    if not os.path.isdir(repo):
        print(f"Error: not a directory: {repo}", file=sys.stderr)
        sys.exit(1)

    lang = _resolve_language(args)
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
    all_files = find_source_files(repo, backend)
    source_count = len([f for f in all_files if not backend.is_test_file(f[0])])
    test_count = len([f for f in all_files if backend.is_test_file(f[0])])
    print(f"  Source files: {source_count}  Test files: {test_count}", file=sys.stderr)

    result = validate(repo, backend, adapter)
    _finalize_single(result, args, lang, adapter, repo_name, source_count)


def _finalize_single(
    result: dict, args: argparse.Namespace, lang: str, adapter: str | None,
    repo_name: str, source_count: int,
) -> None:
    if "error" in result:
        print(f"  ERROR: {result['error']}", file=sys.stderr)
        if result.get("adapter_ok") is False:
            print("  (continuing with file-based analysis only)", file=sys.stderr)

    result.update({
        "language": lang,
        "adapter": adapter,
        "repo": repo_name,
        "source_files": source_count,
    })

    output_json = json.dumps(result, indent=2)
    if args.output:
        with open(args.output, "w") as f:
            f.write(output_json)
        print(f"Results written to {args.output}", file=sys.stderr)
    else:
        print(output_json)

    if "error" not in result:
        _print_single_summary(result, lang, adapter)


def _print_single_summary(result: dict, lang: str, adapter: str | None) -> None:
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


def _skip_reason(repo_path: str, lang: str, adapter: str, name: str) -> dict | None:
    """Skip decision for one suite entry — mirrors the legacy logic exactly."""
    if not (os.path.isdir(os.path.join(repo_path, ".git")) or lang in ("julia", "python")):
        if lang == "julia" and not os.path.isdir(repo_path):
            print(f"  SKIP {name}: fixture not found", file=sys.stderr)
            return {"repo": name, "language": lang, "error": "fixture not found"}
    if not _check_adapter(adapter):
        print(f"  SKIP {name}: adapter not available", file=sys.stderr)
        return {"repo": name, "language": lang, "error": "adapter not available"}
    return None


def _run_suite(repo_path: str, lang: str, adapter: str, name: str) -> dict:
    backend = BACKENDS[lang]()
    print(f"\n  [{lang}] {name}...", file=sys.stderr)
    try:
        r = validate(str(repo_path), backend, adapter)
        r["repo"] = name
        r["language"] = lang
        if "error" not in r:
            c = r
            print(f"    ✅ P:{c['precision']:.1%} R:{c['recall']:.1%} F1:{c['f1']:.1%}  files:{c.get('test_files_analyzed',0)}  fp:{c['false_positives']} fn:{c['false_negatives']}", file=sys.stderr)
        else:
            print(f"    ❌ {r['error']}", file=sys.stderr)
        return r
    except Exception as e:
        print(f"    ❌ error: {e}", file=sys.stderr)
        return {"repo": name, "language": lang, "error": str(e)}


def _aggregate(results: list[dict]) -> dict:
    valid = [r for r in results if "error" not in r]
    if not valid:
        return {"languages_tested": 0}
    total_gt = sum(r["total_imports_ground_truth"] for r in valid)
    total_ad = sum(r["total_imports_adapter"] for r in valid)
    total_matched = sum(r["matched"] for r in valid)
    fp = sum(r["false_positives"] for r in valid)
    fn = sum(r["false_negatives"] for r in valid)
    precision = total_matched / max(total_ad, 1)
    recall = total_matched / max(total_gt, 1)
    f1 = 2 * precision * recall / max(precision + recall, 0.001)
    return {
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


def _print_aggregate(agg: dict) -> None:
    if agg["languages_tested"] <= 0:
        print("No valid results collected.", file=sys.stderr)
        return
    print(f"\n{'='*60}", file=sys.stderr)
    print(f"Multi-language aggregate:", file=sys.stderr)
    print(f"  Languages: {agg['languages_tested']}   GT imports: {agg['total_imports_ground_truth']}", file=sys.stderr)
    print(f"  Aggregate precision: {agg['precision']:.2%}   Recall: {agg['recall']:.2%}   F1: {agg['f1']:.2%}", file=sys.stderr)
    print(f"  Total false positives: {agg['false_positives']}   False negatives: {agg['false_negatives']}", file=sys.stderr)
    print(f"\n  Per language:", file=sys.stderr)
    for r in agg["per_language"]:
        print(f"    {r['language']:10s}  {r['repo']:25s}  P:{r['precision']:.1%}  R:{r['recall']:.1%}  F1:{r['f1']:.1%}  files:{r['test_files']}  fp:{r['fp']} fn:{r['fn']}", file=sys.stderr)


def _run_all():
    """Run validation across all available test repos and aggregate."""
    repo_dir = Path(__file__).resolve().parent.parent.parent
    results = []
    print(f"=== Multi-language import validation ===", file=sys.stderr)

    for repo_path, lang, adapter, name in _test_suites():
        skip = _skip_reason(repo_path, lang, adapter, name)
        if skip is not None:
            results.append(skip)
            continue
        results.append(_run_suite(repo_path, lang, adapter, name))

    output = {
        "schema": "https://testaruda.dev/schemas/import-validation-v2",
        "generated": "2026-07-27",
        "results": results,
        "aggregate": _aggregate(results),
    }

    output_path = Path(repo_dir) / "docs" / "multi-language-import-validation.json"
    with open(output_path, "w") as f:
        json.dump(output, f, indent=2)
    print(f"\nFull results written to {output_path}", file=sys.stderr)
    _print_aggregate(output["aggregate"])


def main():
    parser = _build_parser()
    args = parser.parse_args()

    if args.list_languages:
        _print_language_list()
        sys.exit(0)

    # '--all' mode: run on all available test repos
    if args.all:
        _run_all()
        return

    if not args.repo:
        parser.print_help()
        sys.exit(1)

    _run_single(args)
