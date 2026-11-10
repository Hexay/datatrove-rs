"""datatrove's Trafilatura step on trafilatura-rs (byte-exact Rust trafilatura 1.11), when it is installed.

`extract()` is the only override, so the sandbox, timeout and stats behave exactly as in datatrove. The sandbox child
unpickles this class, so it uses trafilatura-rs under any multiprocessing start method. Configs it does not cover (see
`trafilatura_rs.VERIFIED`) run datatrove's Python path; coverage depends only on the step's fixed config, so one step
never mixes the two and trafilatura's process-global dedup cache stays on one side.
"""

from datatrove.pipeline.extractors import Trafilatura as _DatatroveTrafilatura


class Trafilatura(_DatatroveTrafilatura):
    _covered = True

    def extract(self, text: str) -> str:
        if self._covered:
            import trafilatura_rs

            try:
                return trafilatura_rs.extract(text, favor_precision=self.favour_precision, include_comments=False,
                                              deduplicate=self.deduplicate, **self.kwargs)
            except NotImplementedError:  # raised from the config check, before any extraction state is touched
                self._covered = False
        return super().extract(text)
