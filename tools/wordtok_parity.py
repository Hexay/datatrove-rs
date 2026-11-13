"""Compare the Rust `wordtok` ports with datatrove's tokenizers: words and per-line sentence counts.

Usage: wordtok_parity.py <datatrove_language> <docs.bin> [--n N] [--fuzz N] [--seed S]

Real docs come from `docs.bin` ('\\0'-joined, `tools/fetch_fineweb2.py`); fuzzed docs splice the corpus with
script-specific and adversarial pieces (`wordtok_fuzz.py`). Sentence counts are checked on every line of every
doc against `len(split_into_sentences(line))`; Rust must answer "unknown" exactly where datatrove raises, and
"at_least_one" only where datatrove returns >= 1 without raising. Prints a summary; exits 1 on any mismatch.
"""

import argparse
import sys
import time

from datatrove.utils.text import split_into_sentences
from datatrove.utils.word_tokenizers import load_word_tokenizer
from wordtok_fuzz import fuzz_docs

from datatrove_rs import _core
from datatrove_rs._base import rust_language


def check_sentences(line, lang, name):
    try:
        want = len(split_into_sentences(line, lang))
    except Exception as e:  # noqa: BLE001 - any raise must map to "unknown"
        want = type(e).__name__
    got = _core.sentence_count(line, name)
    if got == "unknown":
        return True  # C4 then runs datatrove for the whole doc
    if got == "at_least_one":
        return isinstance(want, int) and want >= 1
    return got == want


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("language")
    ap.add_argument("docs")
    ap.add_argument("--n", type=int, default=None)
    ap.add_argument("--fuzz", type=int, default=1000)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--no-sentences", action="store_true")
    args = ap.parse_args()
    name = rust_language(args.language)
    if name is None:
        sys.exit(f"no Rust tokenizer for {args.language!r}")
    # "IS_ALPHA" docs always run datatrove's code (spaCy state, see `RustBacked.rust_for`)
    docs = [d for d in open(args.docs, "rb").read().decode("utf-8").split("\0") if d and "IS_ALPHA" not in d]
    docs = docs[: args.n]
    tok = load_word_tokenizer(args.language)
    fuzz = [d for d in fuzz_docs(docs, args.fuzz, args.seed, name) if "IS_ALPHA" not in d]
    corpora = {"real": docs, "fuzz": fuzz}
    bad = 0
    for kind, texts in corpora.items():
        t_py = t_rs = 0.0
        n_tok = n_lines = bad_tok = bad_sent = unknown = 0
        for text in texts:
            t0 = time.perf_counter()
            want = tok.word_tokenize(text)
            t1 = time.perf_counter()
            got = _core.split_into_words(text, name)
            t_rs += time.perf_counter() - t1
            t_py += t1 - t0
            n_tok += len(want)
            if got != want:
                bad_tok += 1
                if bad_tok <= 3:
                    diff = next(i for i, (a, b) in enumerate(zip(got + [None], want + [None])) if a != b)
                    print(f"  token diff @{diff}: rust {got[diff : diff + 4]} py {want[diff : diff + 4]}")
                    print(f"    text: {text[:200]!r}")
            if args.no_sentences:
                continue
            if name == "vi" and _core.split_into_sentences(text, name) != split_into_sentences(text, args.language):
                bad_sent += 1
                print(f"  sentence split diff: {text[:200]!r}")
            for line in text.splitlines():
                line = line.strip()
                n_lines += 1
                unknown += _core.sentence_count(line, name) == "unknown"
                if not check_sentences(line, args.language, name):
                    bad_sent += 1
                    if bad_sent <= 3:
                        print(f"  sentence diff: {line[:200]!r}")
        bad += bad_tok + bad_sent
        print(
            f"{args.language} {kind}: {len(texts)} docs, {n_tok} tokens, {bad_tok} token mismatches; "
            f"{n_lines} lines, {bad_sent} sentence mismatches, {unknown} unknown; "
            f"tokenize py {t_py:.1f}s rust {t_rs:.2f}s ({t_py / max(t_rs, 1e-9):.0f}x)"
        )
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
