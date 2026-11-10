import io
import os
import pickle
import sys

import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "tools"))
import datatrove_rs as rs  # noqa: E402
from fuzz import fuzz_docs  # noqa: E402
from parity import check_filter, check_minhash  # noqa: E402
from parity_cases import FILTER_CASES, MINHASH_CASES, random_filter_cases  # noqa: E402

DOCS = fuzz_docs(400, seed=7)
CASES = FILTER_CASES + random_filter_cases(3, seed=11)


@pytest.mark.parametrize("name,py_cls,rs_cls,kwargs", CASES, ids=[c[0] for c in CASES])
def test_filter_parity(name, py_cls, rs_cls, kwargs):
    report = io.StringIO()
    bad, run_ok = check_filter(name, py_cls, rs_cls, kwargs, DOCS, report)
    assert (bad, run_ok) == (0, True), report.getvalue()[:4000]


@pytest.mark.parametrize("name,cfg", MINHASH_CASES, ids=[c[0] for c in MINHASH_CASES])
def test_minhash_parity(name, cfg):
    report = io.StringIO()
    bad, files_ok = check_minhash(name, cfg, DOCS, report)
    assert (bad, files_ok) == (0, True), report.getvalue()[:4000]


def test_non_english_falls_back_to_python():
    f = rs.GopherQualityFilter(language="deu")
    assert f.rust() is None


def test_pickles_without_rust_handle():
    f = rs.GopherQualityFilter()
    f.rust()
    g = pickle.loads(pickle.dumps(f))
    assert g._rust is None and g.rust() is not None
