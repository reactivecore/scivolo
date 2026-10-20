// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NameOps.scala
//! dotty's `compactify`: a flat class name longer than a class file name allows, hashed.

use md5::{Digest, Md5};

const CLASSFILE_NAME_CHAR_LIMIT: usize = 240;
const MARKER: &str = "$$$$";
const MAX_NAME_LENGTH: usize = {
    let a = CLASSFILE_NAME_CHAR_LIMIT - 6;
    let b = 2 * (CLASSFILE_NAME_CHAR_LIMIT - 6 - 2 * MARKER.len() - 32);
    if a < b { a } else { b }
};

/// `s` itself if short enough, else its first and last quarter around the markers and its MD5
/// (each byte in hex without padding).
pub fn compactify(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= MAX_NAME_LENGTH {
        return s.to_owned();
    }
    let edge = MAX_NAME_LENGTH / 4;
    let prefix: String = chars[..edge].iter().collect();
    let suffix: String = chars[chars.len() - edge..].iter().collect();
    let md5: String = Md5::digest(s.as_bytes()).iter().map(|b| format!("{b:x}")).collect();
    format!("{prefix}{MARKER}{md5}{MARKER}{suffix}")
}
