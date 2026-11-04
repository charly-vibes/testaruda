"""Rust backend: use-statement ground truth + adapter-replica parsing."""

import os

from validate_imports.base import LanguageBackend

_NON_EXTERNAL = {"std", "core", "alloc", "crate", "self", "super"}


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
            deps.add(path.split("::")[0])
        elif t.startswith("use ") and "{" in t:
            path_part = t[4:].split("{")[0].strip().rstrip("::")
            if path_part:
                deps.add(path_part.split("::")[0])
    # Only external crate dependencies (not std/core/alloc/crate/self/super)
    return {d for d in deps if d not in _NON_EXTERNAL and not d.startswith("#")}


def _rust_adapter_imports(content: str) -> set[str]:
    """Replicate the Rust adapter's parse_rust_imports."""
    deps: set[str] = set()
    for line in content.split("\n"):
        t = line.strip()
        if not t.startswith("use "):
            continue
        path = t[4:].rstrip(";").strip()
        parts = path.split("::")
        dep = "::".join(parts[:2]) if len(parts) >= 2 else path
        # Take only the crate-level prefix
        dep = dep.split("::")[0]
        if dep not in _NON_EXTERNAL:
            deps.add(dep)
    return deps
