//! The bytes of the constant pool.

use super::{Entry, Pool};

impl Pool {
    /// The `constant_pool_count` and the entries, or `None` past 65,534 entries.
    pub fn write(&self, out: &mut Vec<u8>) -> Option<()> {
        let count = u16::try_from(self.entries.len() + 1).ok().filter(|&c| c < u16::MAX)?;
        out.extend(count.to_be_bytes());
        for e in &self.entries {
            let (tag, a, b) = match *e {
                Entry::Utf8(ref s) => {
                    let bytes = modified_utf8(s);
                    out.push(1);
                    out.extend(u16::try_from(bytes.len()).ok()?.to_be_bytes());
                    out.extend(bytes);
                    continue;
                }
                Entry::Integer(v) => {
                    out.push(3);
                    out.extend(v.to_be_bytes());
                    continue;
                }
                Entry::Float(v) => {
                    out.push(4);
                    out.extend(v.to_be_bytes());
                    continue;
                }
                Entry::Long(v) => {
                    out.push(5);
                    out.extend(v.to_be_bytes());
                    continue;
                }
                Entry::Double(v) => {
                    out.push(6);
                    out.extend(v.to_be_bytes());
                    continue;
                }
                Entry::MethodHandle(kind, member) => {
                    out.extend([15, kind]);
                    out.extend(member.to_be_bytes());
                    continue;
                }
                Entry::Gap => continue,
                Entry::Class(n) => (7, n, None),
                Entry::String(s) => (8, s, None),
                Entry::Fieldref(c, nt) => (9, c, Some(nt)),
                Entry::Methodref(c, nt) => (10, c, Some(nt)),
                Entry::InterfaceMethodref(c, nt) => (11, c, Some(nt)),
                Entry::NameAndType(n, d) => (12, n, Some(d)),
                Entry::MethodType(d) => (16, d, None),
                Entry::InvokeDynamic(bsm, nt) => (18, bsm, Some(nt)),
            };
            out.push(tag);
            out.extend(a.to_be_bytes());
            out.extend(b.into_iter().flat_map(u16::to_be_bytes));
        }
        Some(())
    }
}

/// `s` in the JVM's modified UTF-8: `\0` in two bytes, supplementary characters as two
/// surrogates of three bytes each.
fn modified_utf8(s: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for &c in s {
        match c {
            0x01..=0x7f => out.push(c as u8),
            0..=0x7ff => out.extend([0xc0 | (c >> 6) as u8, 0x80 | (c & 0x3f) as u8]),
            _ => {
                out.extend([0xe0 | (c >> 12) as u8, 0x80 | ((c >> 6) & 0x3f) as u8, 0x80 | (c & 0x3f) as u8])
            }
        }
    }
    out
}
