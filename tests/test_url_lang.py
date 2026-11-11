"""datatrove_rs URLFilter and LanguageFilter against datatrove's, doc by doc and through `run()` (stats included)."""

import copy
import pickle

import pytest

from datatrove.data import Document

pytest.importorskip("tldextract")
pytest.importorskip("ahocorasick")

URLS = [
    "https://example.com/",
    "http://www.bbc.co.uk/news/world-123",
    "https://0-1sex.com/page",
    "https://shop.0-1sex.com/page",
    "https://forums.bbc.co.uk/porn/thread",
    "https://example.org/xxx-video-free",
    "https://example.org/sexy-escort-girls",
    "https://example.org/hotsexvideo",
    "https://example.org/Porn",
    "https://192.168.0.1:8080/index.html",
    "http://[2001:db8::1]/x",
    "http://localhost/test",
    "https://user:pw@example.com:443/a?b=c#d",
    "https://xn--bcher-kva.example/",
    "https://bücher.example/straße",
    "https://example.com。cn/a",
    "https://EXAMPLE.COM/A",
    "ftp://example.com/file",
    "example.com/no-scheme",
    "/relative/path/",
    "https://blogspot.com/x",
    "https://foo.blogspot.com/x",
    "https://example.com/a\ud800b",
    "-",
    "https://example.com/" + "a" * 3000,
]


def _docs(urls):
    return [Document(text=f"doc {i}", id=str(i), metadata={"url": u}) for i, u in enumerate(urls)]


def _run(step, docs):
    out = [(d.id, d.metadata) for d in step.run(iter(copy.deepcopy(docs)))]
    return out, {k: v.total for k, v in step.stats.stats.items()}


@pytest.fixture(scope="module")
def blocklisted():
    """A few entries of the integrated lists, so the domain/subdomain/url branches are exercised."""
    from datatrove.pipeline.filters import URLFilter

    f = URLFilter()
    f.download_data()
    domains = sorted(f.block_listed_domains)[:: max(1, len(f.block_listed_domains) // 20)]
    urls = sorted(f.block_listed_url)[:: max(1, len(f.block_listed_url) // 20)]
    return [f"https://{d}/" for d in domains] + [f"https://sub.{d}/x" for d in domains[:5]] + urls


@pytest.mark.parametrize("kwargs", [{}, {"soft_word_threshold": 1}, {"extra_domains": ["example.org"],
                                     "extra_urls": ["https://example.com/"], "banned_words": ["news"],
                                     "banned_subwords": ["bbc"], "soft_banned_words": ["thread", "forums"]}])
def test_url_filter_matches_datatrove(kwargs, blocklisted):
    from datatrove.pipeline.filters import URLFilter as Dt

    from datatrove_rs.url_filter import URLFilter

    docs = _docs(URLS + blocklisted)
    ours, theirs = URLFilter(**kwargs), Dt(**kwargs)
    for d in docs:
        assert ours.filter(d) == theirs.filter(d), d.metadata["url"]
    assert ours._lists is not None
    assert _run(URLFilter(**kwargs), docs) == _run(Dt(**kwargs), docs)


HOSTS = ["www.ck", "a.b.ck", "city.kawasaki.jp", "a.city.kawasaki.jp", "x.kawasaki.jp", "foo.blogspot.com",
         "blogspot.com", "a.b.c.blogspot.co.uk", "WWW.Example.CO.UK.", "localhost", "1.2.3.4", "999.1.1.1", "a..com",
         "com", ".com", "s3.amazonaws.com", "x.s3.amazonaws.com", "xn--p1ai", "a.xn--p1ai", "foo.xn--p1ai.com"]
SHAPES = ["http://{}/", "https://user@{}:8080/p?q#f", "//{}", "{}", "http:/{}", "HTTP://{}", "ht!tp://{}/x",
          "http://{}#a/b", "http://{} /x", "http://{}。"]


@pytest.mark.parametrize("private", [False, True])
def test_host_split_matches_tldextract(private):
    from tldextract import TLDExtract

    from datatrove_rs.url_filter import URLFilter

    f = URLFilter()
    f.tldextractor = TLDExtract(include_psl_private_domains=private)
    f.download_data()
    if f._lists.split_host("http://example.com/") is None:
        pytest.skip("tldextract version not reproduced")
    suffixes = sorted(f.tldextractor._get_tld_extractor().tlds())
    hosts = HOSTS + [s.replace("*", "w").lstrip("!") for s in suffixes[::7]]
    hosts += ["sub.d." + h for h in hosts]
    urls = [shape.format(h) for h in hosts for shape in SHAPES] + URLS[:-3]
    decided = 0
    for url in urls:
        got = f._lists.split_host(url)
        if got is not None:
            info = f.tldextractor(url)
            assert got == (info.top_domain_under_public_suffix, info.fqdn), url
            decided += 1
    assert decided > len(urls) * 0.8


def test_url_filter_without_integrated_lists_is_datatrove():
    from datatrove.pipeline.filters import URLFilter as Dt

    from datatrove_rs.url_filter import URLFilter

    kwargs = {"use_integrated_lists": False, "banned_words": ["porn"], "extra_domains": ["bbc.co.uk"]}
    docs = _docs(URLS)
    assert _run(URLFilter(**kwargs), docs) == _run(Dt(**kwargs), docs)


def test_url_filter_pickles_after_loading():
    from datatrove_rs.url_filter import URLFilter

    f = URLFilter()
    f.filter(_docs(URLS[:1])[0])
    g = pickle.loads(pickle.dumps(f))
    assert g._lists is None and not g._downloaded
    assert g.filter(_docs(URLS[2:3])[0]) == (False, "domain")


TEXTS = [
    "The quick brown fox jumps over the lazy dog. " * 5,
    "Le renard brun rapide saute par-dessus le chien paresseux.",
    "Der schnelle braune Fuchs springt über den faulen Hund.\nZweite Zeile.",
    "这是一个中文句子，用于测试语言识别。",
    "hello",
    "1234 5678",
    "English words mixed avec des mots français und deutsche Wörter",
    "   ",
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
    "Это предложение на русском языке.",
    "\n\n",
]


@pytest.fixture(scope="module")
def lid_model():
    from datatrove.utils.lid import FT176LID

    try:
        return FT176LID(["en"]).model
    except Exception as e:  # noqa: BLE001
        pytest.skip(f"fastText lid.176 model unavailable: {e}")


def _lang_outcome(step, text):
    doc = Document(text=text, id="0")
    try:
        return step.filter(doc), doc.metadata
    except Exception as e:  # noqa: BLE001
        return type(e)


@pytest.mark.parametrize("kwargs", [
    {"languages": ["en"]},
    {"languages": ["en", "fr"], "language_threshold": 0.3},
    {"languages": None},
    {"languages": ["de"], "label_only": True},
    {"languages": ["en"], "keep_top_pairs_threshold": 0.01},
    {"languages": ["en"], "language_threshold": 0.0},
])
def test_language_filter_matches_datatrove(kwargs, lid_model):
    from datatrove.pipeline.filters import LanguageFilter as Dt

    from datatrove_rs.language_filter import LanguageFilter

    ours, theirs = LanguageFilter(**kwargs), Dt(**kwargs)
    ours.model._model = theirs.model._model = lid_model
    fast = kwargs.get("keep_top_pairs_threshold", -1) == -1 and kwargs.get("language_threshold", 0.65) > 0
    assert bool(ours.rust()) == fast
    for text in TEXTS:
        assert _lang_outcome(ours, text) == _lang_outcome(theirs, text), text
    docs = [Document(text=t, id=str(i)) for i, t in enumerate(TEXTS) if t.strip()]
    ours, theirs = LanguageFilter(**kwargs), Dt(**kwargs)
    ours.model._model = theirs.model._model = lid_model
    assert _run(ours, docs) == _run(theirs, docs)


def test_language_filter_skips_full_prediction_on_clear_text(lid_model, monkeypatch):
    from datatrove_rs.language_filter import LanguageFilter

    step = LanguageFilter(languages=["en"])
    step.model._model = lid_model
    monkeypatch.setattr(step.model, "predict", lambda doc: pytest.fail("ran the full 176-label prediction"))
    assert step.filter(Document(text=TEXTS[0], id="0")) is True
