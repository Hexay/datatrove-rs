"""urduhack 1.x imports TensorFlow on `import urduhack`, and 1.1.x can't be imported next to numpy 2 at all. When the
package is installed but not importable (CI installs urduhack 1.0.3 with --no-deps), its unmodified rule-based
splitter, `urduhack/tokenization/eos.py`, is exposed as `urduhack.tokenization` so datatrove's Urdu sentences can
run against the Rust port. See docs/parity.md."""

import importlib.metadata
import importlib.util
import sys
import types


def _expose_urduhack_splitter():
    try:
        import urduhack.tokenization  # noqa: F401
        return
    except ImportError:
        pass
    try:
        path = importlib.metadata.distribution("urduhack").locate_file("urduhack/tokenization/eos.py")
    except importlib.metadata.PackageNotFoundError:
        return
    spec = importlib.util.spec_from_file_location("urduhack.tokenization.eos", path)
    eos = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(eos)

    def sentence_tokenizer(text):
        if not isinstance(text, str):
            raise TypeError("text parameter must be str type.")
        return eos._generate_sentences(text)

    tok = types.ModuleType("urduhack.tokenization")
    tok.eos, tok.sentence_tokenizer = eos, sentence_tokenizer
    pkg = types.ModuleType("urduhack")
    pkg.__path__, pkg.tokenization = [], tok
    sys.modules.update({"urduhack": pkg, "urduhack.tokenization": tok, "urduhack.tokenization.eos": eos})


_expose_urduhack_splitter()
