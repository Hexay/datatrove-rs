//! datatrove's `chunk_text_on_bytes(text, max_chunk_size)`, in code points like the Python original.

/// Byte ranges of the chunks; none for empty text.
pub fn chunks(text: &str, max_chunk_size: usize) -> Vec<(usize, usize)> {
    if text.len() <= max_chunk_size {
        // Every branch below then ends with the whole text as one chunk.
        return if text.is_empty() { Vec::new() } else { vec![(0, text.len())] };
    }
    // offs[k] = byte offset of char k; utf8len(remaining[:k]) = offs[base + k] - offs[base].
    let offs: Vec<usize> = text.char_indices().map(|(i, _)| i).chain(std::iter::once(text.len())).collect();
    let n = offs.len() - 1;
    let max = max_chunk_size as f64;
    let factor = n as f64 / text.len() as f64;
    let increase_by = (max * 0.1).clamp(1.0, 10.0) as usize;
    let mut guess = (max * factor - 10.0).max(1.0) as usize;
    let step_down = (max * 0.001).min(10.0) as usize;
    let mut out = Vec::new();
    let mut base = 0;
    while base < n {
        let left = n - base;
        let bytes = |k: usize| offs[base + k.min(left)] - offs[base];
        if bytes(guess) > max_chunk_size {
            guess = guess.saturating_sub(step_down).max(1);
            continue;
        }
        let mut cut = guess;
        let mut part = guess.min(left);
        while bytes(part) < max_chunk_size && part != left {
            cut = left.min(cut + increase_by);
            part = cut;
        }
        if bytes(part) > max_chunk_size {
            cut -= increase_by;
        }
        let end = base + cut.min(left);
        out.push((offs[base], offs[end]));
        base = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::chunks;

    #[test]
    fn covers_text_within_limit() {
        let text = "あいう".repeat(30_000) + &"ab ".repeat(20_000);
        let c = chunks(&text, 40_000);
        assert_eq!(c.first().unwrap().0, 0);
        assert_eq!(c.last().unwrap().1, text.len());
        assert!(c.windows(2).all(|w| w[0].1 == w[1].0));
        assert!(c.iter().all(|&(a, b)| b - a <= 40_000 && a < b));
    }
}
