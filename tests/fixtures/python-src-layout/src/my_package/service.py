"""Business logic service."""

from .model import Model


def process(model: Model) -> str:
    return f"processed {model.name} = {model.value}"