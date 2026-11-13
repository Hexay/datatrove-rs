"""The three ways to switch a pipeline: patch(), the datatrove_rs.* mirror, and the `python -m datatrove_rs` runner."""

import importlib
import os
import pickle
import subprocess
import sys

import pytest

import datatrove_rs as rs
from datatrove_rs._replacements import replacements


@pytest.fixture
def patched():
    rs.patch()
    yield
    rs.unpatch()


def test_patch_swaps_package_and_defining_module(patched):
    from datatrove.pipeline import filters
    from datatrove.pipeline.filters import gopher_quality_filter

    assert filters.GopherQualityFilter is rs.GopherQualityFilter
    assert gopher_quality_filter.GopherQualityFilter is rs.GopherQualityFilter
    assert filters.C4QualityFilter is rs.C4QualityFilter
    assert filters.LambdaFilter.__module__.startswith("datatrove.")
    assert filters.URLFilter.__module__ == "datatrove_rs.url_filter"


def test_patch_covers_reader(patched):
    from datatrove.pipeline.readers import WarcReader
    from datatrove.pipeline.readers.warc import WarcReader as Defining

    from datatrove_rs.readers import WarcReader as Ours

    assert WarcReader is Ours and Defining is Ours


def test_patch_covers_minhash(patched):
    from datatrove.pipeline.dedup import MinhashDedupCluster, MinhashDedupSignature

    assert MinhashDedupSignature is rs.MinhashDedupSignature
    assert MinhashDedupCluster is rs.MinhashDedupCluster


def test_patch_covers_formatters_and_badwords(patched):
    from datatrove.pipeline.filters import C4BadWordsFilter
    from datatrove.pipeline.formatters import FTFYFormatter, PIIFormatter

    for cls in (C4BadWordsFilter, FTFYFormatter, PIIFormatter):
        assert cls.__module__.startswith("datatrove_rs."), cls


def test_unpatch_restores_originals():
    from datatrove.pipeline import filters

    original = filters.FineWebQualityFilter
    rs.patch()
    rs.patch()  # idempotent
    assert filters.FineWebQualityFilter is rs.FineWebQualityFilter
    rs.unpatch()
    assert filters.FineWebQualityFilter is original
    assert not rs.is_patched()


def test_patched_step_pickles_to_rust_class(patched):
    from datatrove.pipeline.filters import GopherRepetitionFilter

    step = pickle.loads(pickle.dumps(GopherRepetitionFilter()))
    assert type(step) is rs.GopherRepetitionFilter


def test_mirror_swaps_without_touching_datatrove():
    from datatrove.pipeline import filters as dt_filters

    from datatrove_rs.pipeline.filters import GopherQualityFilter, LambdaFilter

    assert GopherQualityFilter is rs.GopherQualityFilter
    assert LambdaFilter is dt_filters.LambdaFilter
    assert dt_filters.GopherQualityFilter is not rs.GopherQualityFilter
    assert not rs.is_patched()


def test_mirror_submodule_attribute_access():
    pipeline = importlib.import_module("datatrove_rs.pipeline")
    assert pipeline.filters.C4QualityFilter is rs.C4QualityFilter
    assert pipeline.filters.gopher_quality_filter.GopherQualityFilter is rs.GopherQualityFilter
    from datatrove.pipeline.readers import JsonlReader

    assert importlib.import_module("datatrove_rs.pipeline.readers").JsonlReader is JsonlReader
    with pytest.raises(AttributeError):
        pipeline.no_such_module  # noqa: B018


def test_mirror_maps_every_replacement():
    from datatrove_rs.pipeline.dedup import MinhashDedupSignature

    assert MinhashDedupSignature is rs.MinhashDedupSignature
    assert set(replacements().values()) >= {rs.GopherQualityFilter, rs.MinhashDedupSignature}


def test_language_detection_does_not_build_spacy_tokenizer(monkeypatch):
    from datatrove.utils import word_tokenizers

    from datatrove_rs import _base

    _base.rust_language.cache_clear()
    monkeypatch.setattr(word_tokenizers.SpaCyTokenizer, "__init__", lambda *a, **k: pytest.fail("built spaCy"))
    try:
        assert _base.rust_language("eng") == _base.rust_language("en") == "en"
        assert _base.rust_language("deu") == "de" and _base.rust_language("zul_Latn") == "tn"
        # Kiwi and Stanza ("hin" without a script is Stanza's) stay on datatrove's code; zh/ja need their packages.
        assert all(_base.rust_language(x) is None for x in ("kor", "hin"))
        assert _base.rust_language("cmn") in ("zh", None) and _base.rust_language("jpn") in ("ja", None)
        assert (_base.rust_language("vie"), _base.rust_language("tha")) == ("vi", "thai")
        assert (_base.rust_language("hin_Deva"), _base.rust_language("urd_Arab")) == ("indic:hi", "indic:ur")
    finally:
        _base.rust_language.cache_clear()


def test_language_detection_for_tokenizer_instances():
    from datatrove.utils.word_tokenizers import SpaCyTokenizer, WhitespaceTokenizer

    from datatrove_rs import _base

    for code in _base.rust_languages():
        assert _base.rust_language(SpaCyTokenizer(code)) == code
    assert _base.rust_language(SpaCyTokenizer("en", config={"nlp": {}})) is None
    assert _base.rust_language(WhitespaceTokenizer()) is None


def test_is_alpha_docs_use_rust_without_spacy(monkeypatch):
    from datatrove_rs import _base

    f = rs.GopherQualityFilter()
    assert f.rust_for("x IS_ALPHA y") is None  # spaCy installed in the test env: datatrove handles it
    monkeypatch.setattr(_base, "spacy_installed", lambda: False)
    assert f.rust_for("x IS_ALPHA y") is f.rust()


def test_runner_patches_unmodified_script(tmp_path):
    script = tmp_path / "pipeline.py"
    script.write_text(
        "import sys\n"
        "from datatrove.pipeline.filters import GopherQualityFilter\n"
        "print(GopherQualityFilter.__module__, sys.argv[1:])\n"
    )
    out = subprocess.run(
        [sys.executable, "-m", "datatrove_rs", str(script), "a", "b"],
        capture_output=True,
        text=True,
        check=True,
        env={**os.environ, "DATATROVE_RS": "0"},
    ).stdout
    assert out.strip() == "datatrove_rs.filters ['a', 'b']"
