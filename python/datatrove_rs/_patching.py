"""`patch()`: make datatrove's own import paths hand out the Rust-backed classes, so pipelines need no edits.

Call it before building the pipeline. It swaps attributes in every loaded `datatrove.*` module (catching
`from ... import X` copies taken earlier) and eagerly imports the modules that export replaced classes, so later
imports see the replacements too. The replacements keep `__module__ = "datatrove_rs.*"`, so pickled pipelines
unpickle to them on executor workers whether or not the worker patched.
"""

import importlib
import sys

from datatrove_rs._replacements import replacements

# Modules whose import binds a replaced class; importing them up front lets the swap reach every later importer.
_EXPORTERS = ("datatrove.pipeline.filters", "datatrove.pipeline.dedup", "datatrove.pipeline.extractors")

_swapped: list[tuple[object, str, type]] = []


def patch() -> None:
    """Idempotent. Datatrove modules imported after this call also get the replacements."""
    for name in _EXPORTERS:
        try:
            importlib.import_module(name)
        except ImportError:
            pass
    table = replacements()
    for mod_name, mod in list(sys.modules.items()):
        if not mod_name.startswith("datatrove.") or mod is None:
            continue
        for attr, value in list(vars(mod).items()):
            new = table.get(value) if isinstance(value, type) else None
            if new is not None:
                setattr(mod, attr, new)
                _swapped.append((mod, attr, value))


def unpatch() -> None:
    """Restore every attribute `patch` swapped."""
    while _swapped:
        mod, attr, original = _swapped.pop()
        setattr(mod, attr, original)


def is_patched() -> bool:
    return bool(_swapped)
