"""datatrove_rs.extractors.Trafilatura (trafilatura-rs-backed); skipped when trafilatura-rs is not installed."""

import pickle

import pytest

pytest.importorskip("trafilatura_rs")
pytest.importorskip("trafilatura")

from datatrove.pipeline.extractors import Trafilatura as DtTrafilatura  # noqa: E402

import datatrove_rs as rs  # noqa: E402
from datatrove_rs.extractors import Trafilatura  # noqa: E402

PARA = "<p>{} The committee met on Tuesday to review the proposal, and after a long discussion it was approved.</p>"
PAGES = [
    "<html><head><title>T</title></head><body><nav>Home | About</nav><article>"
    + "".join(PARA.format(f"Paragraph {i}.") for i in range(6))
    + "</article><footer>Cookie policy</footer></body></html>",
    "<html><body><div class='content'><h1>Title</h1>" + PARA.format("Only one.") + "<table><tr><td>a</td><td>b</td>"
    "</tr></table></div><div id='comments'><p>nice post</p></div></body></html>",
    "<html><body><p>too short</p></body></html>",
]


@pytest.mark.parametrize("kwargs", [dict(favour_precision=True, deduplicate=False), dict(favour_precision=False)])
def test_matches_datatrove(kwargs):
    ours, theirs = Trafilatura(**kwargs), DtTrafilatura(**kwargs)
    for html in PAGES:
        assert ours.extract(html) == theirs.extract(html)


def test_uncovered_config_falls_back_to_python():
    step = Trafilatura(favour_precision=True, include_links=True)
    step.extract(PAGES[0])
    assert step._covered is False


def test_patch_and_mirror_replace_the_step():
    from datatrove_rs.pipeline.extractors import Trafilatura as Mirrored

    assert Mirrored is Trafilatura
    rs.patch()
    try:
        from datatrove.pipeline.extractors import Trafilatura as Patched

        assert Patched is Trafilatura
        assert type(pickle.loads(pickle.dumps(Patched()))) is Trafilatura
    finally:
        rs.unpatch()
