"""Adversarial Chinese/Japanese docs for the jieba and SudachiPy ports.

Usage: cjk_fuzz.py <out.bin> <n> <zh|ja> [seed]   ('\\0'-joined, like the FineWeb-2 docs.bin files)

Fragments target each tokenizer's edges: jieba's Han/ASCII block regexes, HMM runs of unknown chars, "\\r\\n",
Python vs regex whitespace; Sudachi's input normalisation (prolonged sound marks, yomigana brackets, half/full width,
numerals), OOV grouping, whitespace runs; datatrove's 40,000-byte Japanese chunks and spaCy's sentencizer.
"""

import random
import sys

SPACES = [" ", "  ", "   ", "\t", "\n", "\n\n", "\r\n", "\r", "　", "\xa0", " ", "\x1c", "\x85", " \n", "\n "]
PUNCT = list("。，、！？：；“”‘’（）《》【】「」『』・….,!?;:()[]\"'-–—~〜") + ["...", "……", "——", "!!", "?!"]
COMMON = [
    "C++", "c#", "a&b", "x_y", "50%", "3.14", "3.14%", "1.5.6", "v2.0", "-", "--", "+", "#", "&", "_", "%", ".",
    "http://example.com/a_b?c=1", "user@example.com", "2024年", "12月31日", "１２３", "ＡＢＣ", "😀", "👨‍👩‍👧",
    "café", "naïve", "한국어", "Привет", "IS", "ALPHA", "é", "​", "﻿", "\U0002A6A5", "鿖", "㐀",
]
ZH = [
    "我们", "中华人民共和国", "北京大学", "的", "了", "是", "在", "研究生命起源", "结婚的和尚未结婚的", "他来到了网易杭研大厦",
    "小明硕士毕业于中国科学院计算所", "长春市长春节讲话", "乒乓球拍卖完了", "工信处女干事每月经过下属科室", "李小福是创新办主任",
    "蔡英文", "鬱", "龘", "氵", "囧", "槑", "朤", "粵語", "廣東話", "唔該", "冇", "嘅", "咗", "侬好", "阿拉", "儂",
    "一二三四五六七八九十", "百分之五十", "ＡＢＣ牌", "iPhone15", "5G网络", "B超", "T恤", "卡拉OK",
]
JA = [
    "私", "は", "東京", "に", "住んで", "います", "すもももももももものうち", "外国人参政権", "東京都庁", "国会議事堂",
    "カタカナ", "ｶﾀｶﾅ", "ﾊﾟｿｺﾝ", "コンピューター", "コンピュータ〜", "ラーメン", "ら〜めん", "すごーい", "すご〜い", "ヴァイオリン",
    "東京(とうきょう)", "漢字（かんじ）", "明日(あした)", "(笑)", "（注）", "１，０００円", "1,000円", "100万円", "三千五百",
    "二〇二四年", "令和6年", "㍻", "①", "ⅱ", "が", "葛\U000E0100", "髙橋", "﨑", "々", "ゝ", "ゞ", "ヽ", "〃",
    "ーー", "ｰ", "〰", "⁓", "…", "‥", "＠", "＃", "ｗｗｗ", "www", "ＷＥＢ", "ＮＨＫ", "ＪＲ東日本", "スマホ", "ググる",
]


def fragment(rng, words):
    r = rng.random()
    if r < 0.45:
        return rng.choice(words)
    if r < 0.6:
        return "".join(chr(rng.randint(0x4E00, 0x9FD5)) for _ in range(rng.randint(1, 4)))
    if r < 0.72:
        return rng.choice(PUNCT)
    if r < 0.85:
        return rng.choice(SPACES)
    return rng.choice(COMMON)


def fuzz_docs(code, n, seed=0):
    rng = random.Random(seed)
    words = ZH if code == "zh" else JA
    docs = []
    for i in range(n):
        parts = [fragment(rng, words) for _ in range(rng.randint(1, 120))]
        doc = "".join(parts)
        if code == "ja" and i % 15 == 0:
            # Long docs cross datatrove's 40,000-byte chunk boundaries, some with spaces near them.
            unit = doc or "テスト"
            doc = (unit + rng.choice(["", " ", "  ", "\n", "。"])) * rng.randint(1, 150_000 // len(unit.encode()) + 1)
        docs.append(doc)
    return docs


if __name__ == "__main__":
    out, n, code = sys.argv[1], int(sys.argv[2]), sys.argv[3]
    docs = fuzz_docs(code, n, int(sys.argv[4]) if len(sys.argv) > 4 else 0)
    with open(out, "wb") as f:
        f.write("\0".join(docs).encode("utf-8"))
    print(f"wrote {len(docs)} docs to {out}")
