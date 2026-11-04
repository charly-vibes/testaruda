"""Python backend: ast ground truth + adapter-replica parsing."""

import ast
import os

from validate_imports.base import LanguageBackend


class PythonBackend(LanguageBackend):

    @property
    def name(self) -> str: return "python"

    @property
    def source_extensions(self) -> tuple[str, ...]: return (".py",)

    def is_test_file(self, rel_path: str) -> bool:
        base = os.path.basename(rel_path)
        return (
            base.startswith("test_")
            or base.endswith("_test.py")
            or "/test_" in rel_path
            or "/tests/" in rel_path
        )

    def extract_imports_ast(self, content: str, file_path: str, repo_root: str) -> set[str]:
        return _py_ast_imports(content, str(file_path), repo_root)

    def extract_imports_adapter(self, content: str, file_path: str, repo_root: str) -> set[str]:
        """Replicate the Rust adapter's parse_python_imports."""
        return _py_adapter_imports(content, file_path)


def _base_package_parts(file_path: str) -> list[str]:
    module_path = file_path.replace(".py", "").replace("/", ".").lstrip(".")
    # Parent package as a list of dotted segments (testaruda-rpqs)
    return module_path.split(".")[:-1]


def _resolve_relative_import(
    level: int, mod: str, base_package_parts: list[str]
) -> str | None:
    """Resolve a relative import to an absolute module path (or None if it
    escapes above the repo root)."""
    parts = list(base_package_parts)
    levels_up = max(0, level - 1)
    if levels_up >= len(parts):
        return None  # above root
    parts = parts[:len(parts) - levels_up]
    if mod:
        parts.append(mod)
    return ".".join(parts)


def _py_ast_imports(content: str, file_path: str, repo_root: str) -> set[str]:
    """Extract Python imports via ast — ground truth."""
    imports: set[str] = set()
    try:
        tree = ast.parse(content, filename=file_path)
    except SyntaxError:
        return imports

    # Derive base package from file path (same logic as adapter)
    rel_path = file_path.replace(repo_root, "").lstrip("/")
    module_path = rel_path.replace(".py", "").replace("/", ".").lstrip(".")
    base_package_parts = module_path.split(".")[:-1]

    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                imports.add(alias.name.split(".")[0])
        elif isinstance(node, ast.ImportFrom):
            imports |= _py_ast_importfrom(node, base_package_parts)
    return imports


def _py_ast_importfrom(node: ast.ImportFrom, base_package_parts: list[str]) -> set[str]:
    level = node.level or 0
    mod = node.module or ""
    if level == 0:
        # Absolute import
        return {mod.split(".")[0]}
    # Relative import — resolve to absolute module path
    resolved = _resolve_relative_import(level, mod, base_package_parts)
    return {resolved.split(".")[0]} if resolved else set()


def _py_parse_import_line(line: str, base_parts: list[str]) -> set[str]:
    # testaruda-wpil: strip trailing comments (mirrors the Rust adapter's
    # parse_import_line) so comment text never leaks into module names
    t = line.split("#", 1)[0].strip()
    if t.startswith("import "):
        m = t[7:].split(" as ")[0].strip()
        return {m.split(".")[0]} if m else set()
    if t.startswith("from "):
        return _py_parse_from(t[5:], base_parts)
    return set()


def _py_parse_from(rest: str, base_parts: list[str]) -> set[str]:
    if rest.startswith("."):
        return _py_from_relative(rest, base_parts)
    m = rest.split(" import ")[0].strip()
    return {m.split(".")[0]} if m else set()


def _py_from_relative(rest: str, base_parts: list[str]) -> set[str]:
    dot_count = len(rest) - len(rest.lstrip("."))
    ad = rest[dot_count:].strip()
    mp = ad.split(" import ")[0].strip() if " import " in ad else ""
    parts = list(base_parts)
    up = max(0, dot_count - 1)
    if up >= len(parts):
        return set()
    parts = parts[:len(parts) - up]
    if mp:
        parts.append(mp)
    return {parts[0]} if parts else set()  # top-level crate


def _py_adapter_imports(content: str, file_path: str) -> set[str]:
    """Replicate the Rust adapter's parse_python_imports."""
    deps: set[str] = set()
    base_parts = _base_package_parts(file_path)
    for line in content.split("\n"):
        deps |= _py_parse_import_line(line, base_parts)
    return deps
