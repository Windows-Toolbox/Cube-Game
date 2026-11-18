//! Numbers as text the way the Python version wrote them, so the texts in the game stay the same: `{:g}`,
//! `{:+g}`, `{:.3g}`, `str(float)` and `json.dumps`.

use serde_json::Value;

/// Python's `format(v, ".{prec}g")`: `prec` significant digits, no trailing zeros, scientific outside 1e-4 … 1e`prec`.
pub fn g_prec(v: f64, prec: usize) -> String {
    if v.is_nan() {
        return "nan".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let p = prec.max(1);
    if v == 0.0 {
        return if v.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    // The exponent after rounding to p digits.
    let sci = format!("{:.*e}", p - 1, v);
    let (mantissa, exp) = sci.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if exp < -4 || exp >= p as i32 {
        let m = strip(mantissa);
        format!("{m}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        strip(&format!("{v:.decimals$}"))
    }
}

fn strip(s: &str) -> String {
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s.to_string() }
}

/// `{:g}`
pub fn g(v: f64) -> String {
    g_prec(v, 6)
}

/// `{:+g}`
pub fn plus_g(v: f64) -> String {
    let s = g(v);
    if s.starts_with('-') { s } else { format!("+{s}") }
}

/// `{:+.{prec}g}`
pub fn plus_g_prec(v: f64, prec: usize) -> String {
    let s = g_prec(v, prec);
    if s.starts_with('-') { s } else { format!("+{s}") }
}

/// `str(x)` of a Python float: like Rust's shortest form, but a whole number keeps its `.0`.
pub fn float(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e16 { format!("{v:.1}") } else { format!("{v}") }
}

/// What Python printed for a number from a data file: an int as an int, a float as a float.
pub fn num(v: &Value) -> String {
    match v {
        Value::Number(n) if n.is_f64() => float(n.as_f64().unwrap_or(0.0)),
        Value::Number(n) => n.to_string(),
        other => dumps(other),
    }
}

/// `json.dumps(v)`: like serde_json, but with Python's spaces after `,` and `:`.
pub fn dumps(v: &Value) -> String {
    match v {
        Value::Array(items) => format!("[{}]", items.iter().map(dumps).collect::<Vec<_>>().join(", ")),
        Value::Object(map) => format!("{{{}}}", map.iter().map(|(k, v)| format!("{}: {}", Value::String(k.clone()), dumps(v))).collect::<Vec<_>>().join(", ")),
        Value::Number(n) if n.is_f64() => float(n.as_f64().unwrap_or(0.0)),
        Value::Bool(b) => if *b { "true".into() } else { "false".into() },
        other => other.to_string(),
    }
}

/// `round(v, digits)` – Python rounds the decimal value exactly; close enough here: through the shortest text.
pub fn round_to(v: f64, digits: i32) -> f64 {
    let k = 10f64.powi(digits);
    let r = (v * k).round() / k;
    if (v * k - (v * k).trunc()).abs() == 0.5 { crate::engine::geom::py_round(v * k) / k } else { r }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_python() {
        assert_eq!(g(1.0), "1");
        assert_eq!(g(0.5), "0.5");
        assert_eq!(g(1.05), "1.05");
        assert_eq!(g(1234567.0), "1.23457e+06");
        assert_eq!(g(0.00001), "1e-05");
        assert_eq!(g_prec(3.14159, 3), "3.14");
        assert_eq!(g_prec(880.0, 4), "880");
        assert_eq!(g_prec(0.04, 4), "0.04");
        assert_eq!(plus_g(2.0), "+2");
        assert_eq!(plus_g(-0.5), "-0.5");
        assert_eq!(float(7.0), "7.0");
        assert_eq!(float(0.1), "0.1");
        assert_eq!(dumps(&serde_json::json!([1, 2.5, "a"])), "[1, 2.5, \"a\"]");
        assert_eq!(num(&serde_json::json!(7)), "7");
        assert_eq!(num(&serde_json::json!(7.0)), "7.0");
    }
}
