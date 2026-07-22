"""Root-level conftest: shared fixtures for all tests."""

import pytest


@pytest.fixture
def sample_model():
    from my_package.model import Model
    return Model(name="test", value=42)