"""`datatrove_rs.<path>` mirrors `datatrove.<path>` for every datatrove module, with Rust-backed classes swapped in.

`from datatrove_rs.pipeline.filters import GopherQualityFilter, URLFilter` yields the Rust filter and datatrove's
own URLFilter. Mirrors are copies of the datatrove module's namespace, so datatrove itself is left untouched
(unlike `patch()`). Names that exist as real datatrove_rs modules (`filters`, `minhash`, ...) are not mirrored.
"""

import importlib
import importlib.abc
import importlib.util
import sys
import types

from datatrove_rs._replacements import replacements

_PREFIX = "datatrove_rs."
_SKIP = {"__name__", "__spec__", "__loader__", "__package__", "__path__", "__file__", "__cached__", "__builtins__"}


def _source_name(fullname: str) -> str:
    return "datatrove." + fullname[len(_PREFIX) :]


class _MirrorFinder(importlib.abc.MetaPathFinder, importlib.abc.Loader):
    """Appended to sys.meta_path, so real datatrove_rs modules are always found first."""

    def find_spec(self, fullname, path, target=None):
        if not fullname.startswith(_PREFIX):
            return None
        try:
            found = importlib.util.find_spec(_source_name(fullname))
        except ModuleNotFoundError:
            return None
        return importlib.util.spec_from_loader(fullname, self, is_package=True) if found else None

    def create_module(self, spec):
        return types.ModuleType(spec.name)

    def exec_module(self, module):
        source = importlib.import_module(_source_name(module.__name__))
        table = replacements()
        for name, value in vars(source).items():
            if name in _SKIP:
                continue
            # Submodules resolve lazily to their mirrors via __getattr__, not to datatrove's originals.
            if isinstance(value, types.ModuleType) and value.__name__ == f"{source.__name__}.{name}":
                continue
            swapped = table.get(value) if isinstance(value, type) else None
            setattr(module, name, swapped or value)
        module.__path__ = []

        def __getattr__(name, _mirror=module.__name__):
            try:
                return importlib.import_module(f"{_mirror}.{name}")
            except ModuleNotFoundError as e:
                if e.name != f"{_mirror}.{name}":
                    raise  # a dependency of the datatrove module is missing; say which
                raise AttributeError(f"module {_mirror!r} has no attribute {name!r}") from None

        module.__getattr__ = __getattr__


def install() -> None:
    if not any(isinstance(f, _MirrorFinder) for f in sys.meta_path):
        sys.meta_path.append(_MirrorFinder())
