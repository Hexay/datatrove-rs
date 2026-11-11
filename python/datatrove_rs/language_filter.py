"""datatrove's LanguageFilter without asking fastText for all 176 labels per document.

datatrove predicts with `k=-1, threshold=0.0` and builds a dict of every label, but only needs the best label and
whether a requested language scores above `language_threshold`. Asking fastText for labels scoring at least half the
threshold returns the same best label and score (the list is sorted, and scores don't depend on the threshold), and
every requested language that could pass; fastText also prunes its hierarchical softmax below the threshold. When the
top label is close to that floor or tied, when top pairs are kept, or for unusual settings, datatrove's own
prediction runs.
"""

from datatrove.pipeline.filters import language_filter as dt
from datatrove.utils.lid import FT176LID, GlotLID

from datatrove_rs._run import FastRun


class LanguageFilter(FastRun, dt.LanguageFilter):
    def rust(self):
        """For `FastRun`: truthy when `filter` can use the thresholded prediction."""
        return True if self._floor() else None

    def _doc_filter(self, rs, counters):
        return self.filter

    def _floor(self) -> float | None:
        thr, model = self.language_threshold, self.model
        ok = (
            type(model) in (FT176LID, GlotLID)
            and model.k == -1
            and self.keep_top_pairs_threshold == -1
            and type(thr) in (int, float)
            and 0 < thr <= 1
            and (self.languages or self.languages is None)
        )
        return thr / 2 if ok else None

    def _predict(self, doc):
        """`self.model.predict(doc)`, with requested languages scoring at most the floor reported as 0.0."""
        floor, lid = self._floor(), self.model
        if floor is None:
            return lid.predict(doc)
        preds = lid.model.f.predict(doc.text.replace("\n", " ") + "\n", -1, floor, "strict")
        if not preds or preds[0][0] <= floor * 1.001 or (len(preds) > 1 and preds[0][0] == preds[1][0]):
            return lid.predict(doc)
        pairs = {label.split("__")[2]: prob for prob, label in preds}
        best = next(iter(pairs.items()))
        return best, {lang: pairs.get(lang, 0.0) for lang in lid.languages} if lid.languages else pairs

    def filter(self, doc):
        best_lang_pair, lang_pairs = self._predict(doc)
        lang, lang_score = best_lang_pair
        if self.backend == "glotlid":
            lang, script = lang.split("_")
            doc.metadata["language_script"] = script
        doc.metadata["language"] = lang
        doc.metadata["language_score"] = lang_score
        if self.keep_top_pairs_threshold != -1:
            for key, value in lang_pairs.items():
                if value > self.keep_top_pairs_threshold:
                    doc.metadata[f"top_language_{key}_score"] = value
        return (
            self.label_only
            or (self.languages and any(score > self.language_threshold for score in lang_pairs.values()))
            or (self.languages is None and lang_score > self.language_threshold)
        )
