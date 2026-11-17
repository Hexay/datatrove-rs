# The `IS_ALPHA` token

datatrove's output on a doc containing the token `IS_ALPHA` is not a function of the doc. This page explains why,
what datatrove-rs does about it, and how the parity harness keeps those docs comparable.

## Cause (spaCy 3.8 + preshed 3.0)

- `IS_ALPHA` is spaCy symbol 1: `StringStore` maps the string to orth 1 instead of a hash, so its lexeme is stored in
  the vocab's `_by_orth` `PreshMap` under key 1.
- Key 1 is preshed's `DELETED_KEY`, kept in a side slot. `map_clear` resets that slot's "set" flag, but `map_get`
  returns the slot's value without checking the flag.
- datatrove tokenizes inside `memory_zone`. The first `IS_ALPHA` token creates a transient lexeme; at the end of the
  zone spaCy clears the key and frees the zone's memory, but `_by_orth.get(1)` keeps returning the freed pointer.
- Every later `IS_ALPHA` token in that vocab reads freed memory in `Vocab.get`. Usually `assert lex.orth in
  self.strings` fails and datatrove's `_do_tokenize` retokenizes the text with `IS_ALPHA` deleted. When the slot has
  been reused so that the field reads 1, the token is accepted, and its text is whatever that memory holds when it
  is read later: `IS_ALPHA`, or another symbol's name (`IS_TITLE`, `LIKE_URL`).

Which outcome happens depends on the process's allocation history, not on the doc. With the same docs in one process,
datatrove's own `SentenceDedupSignature` wrote different signature files on its first and second run, and
`sent_tokenize` returned both `['IS_ALPHA is a spaCy attribute name.', 'IS_ALPHA again.']` and `['is a spaCy
attribute name.', 'again.']` for the same text. This happens with datatrove 0.10.1 and main alike: the bug is in
preshed. It is fixed upstream in explosion/preshed#58 (open, not yet released as of 2026-11).

## What datatrove-rs does

- With spaCy installed, any doc containing `IS_ALPHA` runs datatrove's own code on datatrove's shared tokenizer
  (`RustBacked.rust_for`). The first `IS_ALPHA` doc of a process is deterministic and matches exactly. For later ones
  no implementation can promise datatrove's output, since datatrove itself doesn't produce one reliably: the Rust
  path calls spaCy on fewer docs, so the freed memory can hold something else, just as it can between two datatrove
  runs with different worker counts, step order or preceding steps.
- Without spaCy, datatrove could not run at all, and Rust handles these docs as spaCy would on first sight.

## Parity harness

`settle_is_alpha_state` (`tools/parity_ref.py`) gives datatrove's shared tokenizer for the language a fresh vocab with
a permanent `IS_ALPHA` lexeme, added outside any memory zone. Key 1 then always points at live memory, `IS_ALPHA`
tokenizes as on first sight on both sides, and the comparison is deterministic.
`test_is_alpha_reference_is_deterministic` in `tests/test_sentence_dedup.py` checks that state.
