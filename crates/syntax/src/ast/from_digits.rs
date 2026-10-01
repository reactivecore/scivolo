//! Numbers from their digits, as `scala.util.FromDigits`.

pub fn int(digits: &str, radix: u32) -> Result<i32, &'static str> {
    integer(digits, radix, i32::MAX as i64).map(|v| v as i32)
}

pub fn long(digits: &str, radix: u32) -> Result<i64, &'static str> {
    integer(digits, radix, i64::MAX)
}

fn integer(digits: &str, radix: u32, limit: i64) -> Result<i64, &'static str> {
    let (negated, rest) = match digits.as_bytes().first() {
        Some(b'-') => (true, &digits[1..]),
        Some(b'+') => (false, &digits[1..]),
        _ => (false, digits),
    };
    if rest.is_empty() {
        return Err("malformed number literal");
    }
    let (radix, divider) = (radix as i64, if radix == 10 { 1 } else { 2 });
    let mut value: i64 = 0;
    for ch in rest.chars() {
        let d = ch.to_digit(36).map_or(-1, |d| d as i64);
        if d < 0 || radix <= d {
            return Err("malformed number literal");
        }
        let too_large = value < 0
            || limit / (radix / divider) < value
            || limit - (d / divider) < value * (radix / divider)
                && !(negated && limit == value.wrapping_mul(radix).wrapping_sub(1).wrapping_add(d));
        if too_large {
            return Err("number too large");
        }
        value = value.wrapping_mul(radix).wrapping_add(d);
    }
    Ok(if negated { value.wrapping_neg() } else { value })
}

fn is_zero(digits: &str) -> bool {
    let m = digits.strip_prefix('-').unwrap_or(digits);
    let m = m.trim_end_matches(['f', 'F', 'd', 'D']);
    let mantissa = m.split(['e', 'E']).next().unwrap_or("");
    !mantissa.is_empty() && mantissa.chars().all(|c| c == '0' || c == '.')
}

pub fn float(digits: &str) -> Result<f32, &'static str> {
    let x: f32 = digits.trim_end_matches(['f', 'F', 'd', 'D']).parse().map_err(|_| "malformed number literal")?;
    if x.is_infinite() {
        return Err("number too large");
    }
    if x == 0.0 && !is_zero(digits) { Err("number too small") } else { Ok(x) }
}

pub fn double(digits: &str) -> Result<f64, &'static str> {
    let x: f64 = digits.trim_end_matches(['f', 'F', 'd', 'D']).parse().map_err(|_| "malformed number literal")?;
    if x.is_infinite() {
        return Err("number too large");
    }
    if x == 0.0 && !is_zero(digits) { Err("number too small") } else { Ok(x) }
}
