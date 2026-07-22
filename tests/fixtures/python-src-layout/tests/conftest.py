"""Test-level conftest: additional fixtures for test suite."""

import pytest


@pytest.fixture
def other_model():
    from my_package.model import Model
    return Model(name="other", value=99)