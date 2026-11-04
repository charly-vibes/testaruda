"""Language backend base class."""

from abc import ABC, abstractmethod


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
