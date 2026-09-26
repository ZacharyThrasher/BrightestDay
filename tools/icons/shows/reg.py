"""Entry registry: modules decorate builder functions with @entry; each returns (names, recipe, shape|None, note)."""
from slib import R, S

E = []


def entry(f):
    E.append(f)
    return f


def simple(names, recipe, note=None):
    """Register a procedural (shape: None) entry in one line."""
    E.append(lambda: (names, recipe, None, note))


__all__ = ["E", "entry", "simple", "R", "S"]
