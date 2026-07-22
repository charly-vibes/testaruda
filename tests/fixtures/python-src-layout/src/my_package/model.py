"""Data model for the package."""

from dataclasses import dataclass


@dataclass
class Model:
    name: str
    value: int