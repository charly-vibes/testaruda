"""Tests for the data model."""

from my_package.model import Model


def test_model_creation():
    m = Model(name="test", value=1)
    assert m.name == "test"
    assert m.value == 1


def test_model_defaults(sample_model):
    assert sample_model.name == "test"
    assert sample_model.value == 42