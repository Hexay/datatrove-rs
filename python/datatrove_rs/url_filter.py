"""datatrove's URLFilter with the blocklists, word checks and tldextract's host split in Rust.

datatrove loads ~4.6M blocklisted domains into a Python set and tldextract builds two suffix tries on the first
`filter` call (most of the step's time on a 5k-document shard); Rust loads the same files into a compact set and
splits hosts with the same suffix list tldextract loads. The small word lists are still read by datatrove's
`get_list`, so `banned_words`/`soft_banned_words`/`banned_subwords` hold what datatrove's would; only
`block_listed_domains`/`block_listed_url` keep just the user's extras. Hosts the Rust split doesn't reproduce
(non-ASCII, IPv6, punycode) and tldextract versions other than 5.4 go through `self.tldextractor`.
"""

import codecs
import locale
import os
import tarfile

from datatrove.pipeline.filters import url_filter as dt

from datatrove_rs import _core
from datatrove_rs._run import FastRun


def _locale_is_utf8() -> bool:
    """`open()` without an encoding decodes the lists with this codec; the Rust loader only does UTF-8."""
    return codecs.lookup(locale.getpreferredencoding(False)).name == "utf-8"


def _suffix_lists(tx) -> tuple[list[str], list[str]] | None:
    """The suffixes tldextract 5.4's `_get_tld_extractor` builds its trie from: (public + extra, private if the
    extractor includes PSL private domains else []). None when the extractor isn't one the Rust split reproduces."""
    import tldextract
    from tldextract import tldextract as impl

    if not tldextract.__version__.startswith("5.4.") or type(tx) is not impl.TLDExtract:
        return None
    ex = tx._extractor
    if ex is not None:
        public, private, include_private = list(ex.tlds_excl_private), ex.private_tlds, ex.include_psl_private_domains
    else:
        public, private = impl.get_suffix_lists(
            cache=tx._cache,
            urls=tx.suffix_list_urls,
            cache_fetch_timeout=tx.cache_fetch_timeout,
            fallback_to_snapshot=tx.fallback_to_snapshot,
            session=None,
        )
        if not any([public, private, tx.extra_suffixes]):
            return None  # tldextract raises on first use
        public, include_private = list(public) + list(tx.extra_suffixes), tx.include_psl_private_domains
    return public, list(private) if include_private else []


class URLFilter(FastRun, dt.URLFilter):
    _lists = None

    def _rust_applicable(self) -> bool:
        t = self.soft_word_threshold
        return (
            self.use_integrated_lists
            and type(t) in (int, float, bool)
            and abs(t) < 2**53
            and _locale_is_utf8()
        )

    def rust(self):
        """For `FastRun`: truthy when `filter` uses the Rust lists (loaded lazily on the first document)."""
        return True if self._rust_applicable() else None

    def _doc_filter(self, rs, counters):
        return self.filter

    def download_data(self):
        if self._downloaded or not self.use_integrated_lists:
            return
        if not self._rust_applicable():
            return super().download_data()
        download_dir = dt.cached_assets_path(library_name="datatrove", namespace="filters", subfolder="url_filter")
        file_to_lock = os.path.join(download_dir, "url_filterblacklistsv0_3_0.tar.gz")

        def do_extract():
            dt.logger.info("💥 Extracting url filter blacklists...")
            with tarfile.open(os.path.join(dt.ASSETS_PATH, "url_filterblacklistsv0_3_0.tar.gz"), "r:gz") as tar:
                tar.extractall(download_dir)
            dt.logger.info("💥 Extracted url filter blacklists.")

        dt.safely_create_file(file_to_lock, do_extract)
        banned = dt.get_list(dt.ASSETS_PATH, "banned_words.txt", self.banned_words)
        subwords = dt.get_list(dt.ASSETS_PATH, "banned_subwords.txt", self.banned_subwords)
        soft = dt.get_list(dt.ASSETS_PATH, "soft_banned_words.txt", self.soft_banned_words)
        try:
            lists = _core.UrlLists(
                os.path.join(download_dir, "domains"),
                os.path.join(download_dir, "urls"),
                list(self.block_listed_domains),
                list(self.block_listed_url),
                list(banned),
                list(soft),
                list(subwords),
                float(self.soft_word_threshold),
                _suffix_lists(self.tldextractor),
            )
        except ValueError:  # unreadable or non-UTF-8 list: datatrove's loader decides what happens
            return super().download_data()
        self.banned_words, self.banned_subwords, self.soft_banned_words = banned, subwords, soft
        for word in self.banned_subwords:
            self.banned_subwords_automaton.add_word(word, len(self.banned_subwords_automaton))
        self.banned_subwords_automaton.make_automaton()
        self._lists = lists
        self._downloaded = True

    def filter(self, document):
        self.download_data()
        lists = self._lists
        if lists is None:
            return super().filter(document)
        url = document.metadata.get("url")
        assert url, "Document does not have url in its metadata"
        if type(url) is not str:
            return super().filter(document)
        try:
            result = lists.check_url(url)
            if result is None:
                url_info = self.tldextractor(url)
                # tldextract >= 5.3 warns on every `registered_domain` read; same value without the warning
                domain = getattr(url_info, "top_domain_under_public_suffix", None)
                if domain is None:
                    domain = url_info.registered_domain
                result = lists.check(url, domain, url_info.fqdn)
        except UnicodeEncodeError:  # lone surrogates
            return super().filter(document)
        return True if result is True else (False, result)

    def __getstate__(self):
        state = self.__dict__.copy()
        if state.pop("_lists", None) is not None:
            state["_downloaded"] = False  # the domain lists live only in the Rust object
        return state
