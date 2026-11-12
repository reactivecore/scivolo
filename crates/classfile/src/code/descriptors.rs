//! The slots and parts of field and method descriptors.

use crate::kind::Kind;

/// The slots a value of field descriptor `desc` takes, 0 for `V`.
pub fn value_slots(desc: &str) -> i32 {
    Kind::of(desc).slots()
}

/// The parameter descriptors and the result descriptor of method descriptor `d`.
pub fn split_method(d: &str) -> (Vec<&str>, &str) {
    let (b, mut params, mut i) = (d.as_bytes(), Vec::new(), 1);
    while i < b.len() && b[i] != b')' {
        let start = i;
        while b[i] == b'[' {
            i += 1;
        }
        i += if b[i] == b'L' { d[i..].find(';').map_or(1, |e| e + 1) } else { 1 };
        params.push(&d[start..i]);
    }
    (params, d.get(i + 1..).unwrap_or("V"))
}

/// The slots of the parameters and of the result of method descriptor `d`.
pub fn method_slots(d: &str) -> (i32, i32) {
    let (params, result) = split_method(d);
    (params.into_iter().map(value_slots).sum(), value_slots(result))
}
