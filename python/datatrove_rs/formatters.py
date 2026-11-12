import ipaddress
from functools import lru_cache, partial

from datatrove.pipeline.formatters import ftfy as dt_ftfy
from datatrove.pipeline.formatters import pii as dt

from datatrove_rs import _core, ftfy_check
from datatrove_rs._base import DropsRustOnPickle

_NOT_APPLICABLE = False


@lru_cache(maxsize=1)
def ipv4_global_intervals() -> list[tuple[int, bool]]:
    """`(first address, is_global)` per interval between all IPv4 network bounds the running `ipaddress` uses.

    `is_global` only tests membership in those networks, so it is constant inside each interval; reading the table
    from the running Python keeps up with its IANA registry updates.
    """
    nets = {ipaddress.IPv4Network("100.64.0.0/10")}
    for value in vars(ipaddress._IPv4Constants).values():
        values = value if isinstance(value, (list, tuple, set, frozenset)) else [value]
        nets.update(v for v in values if isinstance(v, ipaddress.IPv4Network))
    bounds = {0}
    for net in nets:
        bounds.add(int(net.network_address))
        bounds.add(int(net.broadcast_address) + 1)
    return [(b, ipaddress.IPv4Address(b).is_global) for b in sorted(bounds) if b < 1 << 32]


@lru_cache(maxsize=1)
def _default_patterns() -> tuple[str, str]:
    default = dt.PIIFormatter()
    return default.emails_replacer.regex.pattern, default.ip_replacer.regex.pattern


class PIIFormatter(DropsRustOnPickle, dt.PIIFormatter):
    """Email/IP replacement in Rust (`crates/datatrove-core/src/pii.rs`). The round-robin counters stay on the
    datatrove replacers, so docs that must run in Python (lone surrogates) continue the same sequence."""

    def _build_rust(self):
        emails, ips = self.emails_replacer, self.ip_replacer
        v = ips.validator
        exact = (
            (emails.regex.pattern, ips.regex.pattern) == _default_patterns()
            and emails.validator is None
            and isinstance(v, partial)
            and v.func is dt.public_ip_validator
            and not v.args
            and set(v.keywords) <= {"public_only"}
            and all(r and all(type(x) is str for x in r) for r in (emails.replacements, ips.replacements))
        )
        if not exact:
            return _NOT_APPLICABLE
        return _core.Pii(
            ipv4_global_intervals(),
            not dt.public_ip_validator("01.1.1.1", public_only=False),
            bool(v.keywords.get("public_only", True)),
            list(emails.replacements),
            list(ips.replacements),
        )

    def format(self, text: str) -> str:
        if self._rust is None:
            self._rust = self._build_rust()
        if not self._rust:
            return super().format(text)
        emails, ips = self.emails_replacer, self.ip_replacer
        try:
            new, emails._replace_i, ips._replace_i = self._rust.format(
                text, self.remove_emails, self.remove_ips, emails._replace_i, ips._replace_i
            )
        except UnicodeEncodeError:
            return super().format(text)
        return text if new is None else new


class FTFYFormatter(DropsRustOnPickle, dt_ftfy.FTFYFormatter):
    """Returns the text as is when `ftfy_check` proves ftfy would leave it unchanged; the rest goes through ftfy."""

    def format(self, text: str) -> str:
        if self._rust is None:
            self._rust = ftfy_check.build(self.config) or _NOT_APPLICABLE
        if self._rust:
            check, max_len = self._rust
            if len(text) <= max_len:
                try:
                    if check.first_hit(text) is None:
                        return text
                except UnicodeEncodeError:
                    pass
        return super().format(text)
