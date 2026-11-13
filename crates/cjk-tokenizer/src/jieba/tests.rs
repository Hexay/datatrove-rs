use super::{Hmm, Jieba};

fn s(x: &str) -> String {
    x.to_owned()
}

/// A toy model: every Han char emits only as S, so the HMM cuts unknown runs into single chars.
fn toy() -> Jieba {
    let hmm = Hmm::new(
        vec![(s("B"), -1.0), (s("M"), -3.14e100), (s("E"), -3.14e100), (s("S"), -1.0)],
        vec![
            (s("B"), vec![(s("E"), -0.5), (s("M"), -0.9)]),
            (s("E"), vec![(s("B"), -0.6), (s("S"), -0.8)]),
            (s("M"), vec![(s("E"), -0.3), (s("M"), -1.3)]),
            (s("S"), vec![(s("B"), -0.7), (s("S"), -0.7)]),
        ],
        vec![(s("B"), vec![]), (s("M"), vec![]), (s("E"), vec![]), (s("S"), vec![(s("天"), -1.0), (s("气"), -1.0)])],
        vec![],
    )
    .unwrap();
    let (freq, total) = Jieba::parse_dict("北京 100 ns\n北京大学 50 nt\n大学 80 n\n学 1 n\n").unwrap();
    Jieba::new(freq, total, hmm).unwrap()
}

fn cut(j: &Jieba, text: &str) -> Vec<String> {
    let mut out = Vec::new();
    j.cut(text, &mut out);
    out.iter().map(|&(a, b)| text[a as usize..b as usize].to_owned()).collect()
}

fn tokens(j: &Jieba, text: &str) -> Vec<String> {
    let (mut words, mut out) = (Vec::new(), Vec::new());
    j.tokenize(text, 0, &mut words, &mut out);
    out.iter().map(|&(a, b)| text[a as usize..b as usize].to_owned()).collect()
}

#[test]
fn dictionary_words_and_blocks() {
    let j = toy();
    assert_eq!(cut(&j, "北京大学"), ["北京大学"]);
    assert_eq!(cut(&j, "a\r\nb  c。"), ["a", "\r\n", "b", " ", " ", "c", "。"]);
    assert_eq!(cut(&j, "天气北京"), ["天", "气", "北京"]);
}

#[test]
fn hmm_skip_pieces() {
    // A run of single-char words that isn't a word goes to finalseg, whose non-Han pieces follow `re_skip`.
    assert_eq!(cut(&toy(), "ab1.5%x+#y"), ["ab1.5%", "x", "+#", "y"]);
}

#[test]
fn spacy_alignment() {
    let j = toy();
    assert_eq!(tokens(&j, "a  b"), ["a", " ", "b"]);
    assert_eq!(tokens(&j, " a b "), [" ", "a", "b"]);
    assert_eq!(tokens(&j, "a\n\nb\u{3000}"), ["a", "\n\n", "b", "\u{3000}"]);
}
