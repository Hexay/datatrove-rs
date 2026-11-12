"""datatrove_rs PIIFormatter against datatrove's on adversarial and random text; one instance per side across all
docs, so the round-robin replacement counters are compared too."""

import ipaddress
import pickle
import random

import pytest
from datatrove.pipeline.formatters import pii as dt

from datatrove_rs.formatters import PIIFormatter, ipv4_global_intervals

ADVERSARIAL = [
    "",
    "plain text without anything",
    "mail me at john.doe@example.com or jane@mail.co.uk.",
    "a@b.c x@y.z first@last",
    "é.john@example.com",
    "éjohn@example.com",
    "_john@example.com __x@y.io",
    "ⅫA@b.com ²x@y.com ٣a@b.co",
    "́x@y.com x́@y.com",
    "!!!foo@bar.com #x@y.com -a@b.c",
    "user@[192.168.0.1] user@[300.1.1.1] user@[IPv6:2001:db8::1] user@[abc:]",
    "a..b@c.com .a@b.com a.@b.com",
    "foo@-bar.com foo@bar-.com foo@b-a-r.com foo@bar..com",
    "x" * 500 + "@example.com",
    ("a." * 200) + "@" + ("b." * 200),
    "a" * 3000,
    "@@@ @a.com a@ @",
    "emails: a@b.co,c@d.org;e@f.net",
    "8.8.8.8 and 10.0.0.1 and 192.168.1.1 and 1.1.1.1",
    "1234.5.6.7 1.2.3.4567 01.2.3.4 1.02.3.4 1.2.3.00 0.0.0.0 255.255.255.255 256.1.1.1",
    "100.64.0.1 100.127.255.255 100.128.0.0 192.0.0.9 192.0.0.10 192.0.0.170 192.0.0.172 224.0.0.1",
    "version 1.2.3.4.5.6.7.8 build 999.999.999.999",
    "ip=8.8.4.4:53, host 8.8.8.8/32; 172.15.0.1 172.16.0.1 172.31.255.255 172.32.0.0",
    "mixed admin@8.8.8.8.example.com and admin@[8.8.8.8]",
    "lone surrogate \ud800 near a@b.com and 8.8.8.8",
    "日本語のテキスト user@例え.jp ascii@example.jp 1.1.1.1",
]


def _pair(**kw):
    return dt.PIIFormatter(**kw), PIIFormatter(**kw)


def _compare(texts, **kw):
    py, rs = _pair(**kw)
    for t in texts:
        assert rs.format(t) == py.format(t), repr(t)
    assert rs.emails_replacer._replace_i == py.emails_replacer._replace_i
    assert rs.ip_replacer._replace_i == py.ip_replacer._replace_i
    assert rs._rust, "Rust path not used"


CONFIGS = [
    {},
    {"only_remove_public_ips": False},
    {"remove_emails": False},
    {"remove_ips": False, "email_replacement": "<EMAIL>"},
    {"email_replacement": ("", "x"), "ip_replacement": ("a", "b", "c")},
]


@pytest.mark.parametrize("kw", CONFIGS)
def test_adversarial(kw):
    _compare(ADVERSARIAL * 2, **kw)


def _fuzz_text(rng: random.Random) -> str:
    pieces = list("abcXYZ019_.-@[]:+!~é́²٣Ⅻ ,;\n") + ["8.8.8.8", "10.0.0.", "192.168.", "255.", "25", "@ex.com"]
    return "".join(rng.choice(pieces) for _ in range(rng.randrange(0, 80)))


@pytest.mark.parametrize("kw", CONFIGS[:2])
def test_fuzz(kw):
    rng = random.Random(0)
    _compare([_fuzz_text(rng) for _ in range(5000)], **kw)


def test_every_ipv4_interval_bound():
    texts = []
    for start, _ in ipv4_global_intervals():
        for ip in (start - 1, start, start + 1):
            if 0 <= ip < 1 << 32:
                texts.append(f"at {ipaddress.IPv4Address(ip)} here")
    rng = random.Random(1)
    texts += [f"{ipaddress.IPv4Address(rng.getrandbits(32))}" for _ in range(20000)]
    _compare(texts)


def test_unusual_config_stays_python():
    rs = PIIFormatter(email_replacement=())
    rs.format("nothing here")
    assert rs._rust is False


def test_pickle_drops_rust():
    rs = PIIFormatter()
    rs.format("a@b.com")
    clone = pickle.loads(pickle.dumps(rs))
    assert clone.format("c@d.com") == "firstname.lastname@example.org"
