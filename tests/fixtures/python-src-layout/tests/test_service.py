"""Tests for the service layer."""

from my_package.model import Model
from my_package.service import process


def test_process():
    result = process(Model(name="x", value=5))
    assert result == "processed x = 5"


def test_process_with_fixture(sample_model, other_model):
    r1 = process(sample_model)
    r2 = process(other_model)
    assert "test" in r1
    assert "other" in r2