//! `str(round(value, digits))`, the histogram bin key `BaseStats.get_kv` builds, with CPython's float rules.

use super::Num;

/// `str(round(value, digits))` for `digits >= 0`.
pub fn histogram_key(value: Num, digits: usize, out: &mut String) {
    out.clear();
    match value {
        // round(int, ndigits >= 0) returns the int unchanged
        Num::Int(v) => {
            use std::fmt::Write;
            let _ = write!(out, "{v}");
        }
        Num::Float(x) => py_repr(py_round(x, digits), out),
    }
}

/// CPython `round(x, digits)`: the correctly rounded decimal string (ties to even, on the exact binary value)
/// parsed back. Rust's exact-precision formatting rounds the same way.
pub fn py_round(x: f64, digits: usize) -> f64 {
    if !x.is_finite() {
        return x;
    }
    format!("{x:.digits$}").parse().expect("formatted float parses")
}

/// CPython `repr(float)`: the shortest round-tripping digits, positional when the decimal exponent is in
/// [-4, 16), else `d.ddde±XX`.
pub fn py_repr(x: f64, out: &mut String) {
    out.clear();
    if x.is_nan() {
        out.push_str("nan");
        return;
    }
    if x.is_sign_negative() {
        out.push('-');
    }
    if x.is_infinite() {
        out.push_str("inf");
        return;
    }
    if x == 0.0 {
        out.push_str("0.0");
        return;
    }
    // `{:e}` gives the shortest length, but on a tie between two shortest candidates it can pick the other one
    // than CPython (the correctly rounded one, ties to even), which `{:.N$e}` yields.
    let shortest = format!("{:e}", x.abs());
    let sig = shortest.split_once('e').expect("{:e} has an exponent").0.replace('.', "").len();
    let sci = format!("{:.*e}", sig - 1, x.abs());
    let (mant, exp) = sci.split_once('e').expect("{:e} has an exponent");
    let digits: String = mant.chars().filter(|&c| c != '.').collect();
    let exp: i32 = exp.parse().expect("{:e} exponent");
    let decpt = exp + 1;
    if decpt <= -4 || decpt > 16 {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let sign = if exp < 0 { '-' } else { '+' };
        out.push_str(&format!("e{sign}{:02}", exp.abs()));
    } else if decpt <= 0 {
        out.push_str("0.");
        out.extend(std::iter::repeat_n('0', (-decpt) as usize));
        out.push_str(&digits);
    } else if decpt as usize >= digits.len() {
        out.push_str(&digits);
        out.extend(std::iter::repeat_n('0', decpt as usize - digits.len()));
        out.push_str(".0");
    } else {
        out.push_str(&digits[..decpt as usize]);
        out.push('.');
        out.push_str(&digits[decpt as usize..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(v: Num, d: usize) -> String {
        let mut s = String::new();
        histogram_key(v, d, &mut s);
        s
    }

    #[test]
    fn matches_python() {
        let cases = [
            (Num::Float(0.0625), 3, "0.062"),
            (Num::Float(0.5625), 3, "0.562"),
            (Num::Float(0.0015), 3, "0.002"),
            (Num::Float(1.0), 3, "1.0"),
            (Num::Float(0.0), 3, "0.0"),
            (Num::Float(2.0 / 3.0), 3, "0.667"),
            (Num::Float(1e-5), 3, "0.0"),
            (Num::Float(1.5e-5), 6, "1.5e-05"),
            (Num::Float(1.5e16), 3, "1.5e+16"),
            (Num::Float(1e15), 3, "1000000000000000.0"),
            (Num::Float(123.4567), 0, "123.0"),
            (Num::Float(2.5), 0, "2.0"),
            (Num::Int(42), 3, "42"),
            // exactly ...031.15625: the two shortest reprs tie
            (Num::Float(700_079_691_031.0 + 0.156_25), 5, "700079691031.1562"),
        ];
        for (v, d, want) in cases {
            assert_eq!(key(v, d), want, "{v:?} {d}");
        }
    }
}
