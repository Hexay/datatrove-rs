"""datatrove's Trafilatura step on trafilatura-rs (byte-exact Rust trafilatura 1.11), when it is installed.

`extract()` is the only override, so the sandbox, timeout and stats behave exactly as in datatrove. The sandbox child
unpickles this class, so it uses trafilatura-rs under any multiprocessing start method. The step runs datatrove's
Python path when the installed trafilatura/lxml differ from what trafilatura-rs reproduces, or when its config is
outside `trafilatura_rs.VERIFIED`. Both depend only on the environment and the step's fixed config, so one step never
mixes the two and trafilatura's process-global dedup cache stays on one side.
"""

from datatrove.pipeline.extractors import Trafilatura as _DatatroveTrafilatura


class Trafilatura(_DatatroveTrafilatura):
    _covered = None

    def _extract_kwargs(self) -> dict:
        return dict(favor_precision=self.favour_precision, include_comments=False, deduplicate=self.deduplicate,
                    **self.kwargs)

    def extract(self, text: str) -> str:
        if self._covered is None:
            import trafilatura_rs

            self._covered = not trafilatura_rs.mismatched_versions() and trafilatura_rs.supports(
                **self._extract_kwargs())
        if not self._covered:
            return super().extract(text)
        import trafilatura_rs

        return trafilatura_rs.extract(text, **self._extract_kwargs())
