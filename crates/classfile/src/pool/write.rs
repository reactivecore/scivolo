//! The bytes of the constant pool.

use super::{Entry, Pool};

impl Pool {
    /// The `constant_pool_count` and the entries, or `None` past 65,534 entries.
    pub fn write(&self, out: &mut Vec<u8>) -> Option<()> {
        let count = u16::try_from(self.entries.len() + 1).ok()?;
        out.extend(count.to_be_bytes());
        for e in &self.entries {
            let (tag, a, b) = match *e {
                Entry::Utf8(ref s) => {
                    let bytes = crate::modified_utf8::encode(s);
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
