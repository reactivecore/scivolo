//! The bytes of a class file.

use crate::{Attribute, ClassFile, Code, MAJOR_VERSION, Member};

impl ClassFile {
    /// The class file's bytes, or why it cannot hold the class.
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        if let Some(e) = &self.error {
            return Err(e.clone());
        }
        let mut out = Vec::new();
        out.extend(0xcafe_babe_u32.to_be_bytes());
        out.extend(0_u16.to_be_bytes());
        out.extend(MAJOR_VERSION.to_be_bytes());
        self.pool.write(&mut out).ok_or("more than 65,534 constants")?;
        for v in [self.access, self.this, self.superclass] {
            out.extend(v.to_be_bytes());
        }
        out.extend(count(self.interfaces.len())?);
        out.extend(self.interfaces.iter().flat_map(|i| i.to_be_bytes()));
        for members in [&self.fields, &self.methods] {
            out.extend(count(members.len())?);
            for m in members {
                member(&mut out, m)?;
            }
        }
        let bootstraps = self.pool.bootstrap_methods().map(|(name, bytes)| Attribute { name, bytes });
        let attrs: Vec<&Attribute> = self.attributes.iter().chain(&bootstraps).collect();
        attributes(&mut out, attrs)?;
        Ok(out)
    }
}

fn member(out: &mut Vec<u8>, m: &Member) -> Result<(), String> {
    for v in [m.access, m.name, m.descriptor] {
        out.extend(v.to_be_bytes());
    }
    attributes(out, &m.attributes)
}

fn attributes<'a>(out: &mut Vec<u8>, attrs: impl IntoIterator<Item = &'a Attribute>) -> Result<(), String> {
    let attrs: Vec<&Attribute> = attrs.into_iter().collect();
    out.extend(count(attrs.len())?);
    for a in attrs {
        out.extend(a.name.to_be_bytes());
        let len = u32::try_from(a.bytes.len()).map_err(|_| "an attribute over 4 GiB")?;
        out.extend(len.to_be_bytes());
        out.extend(&a.bytes);
    }
    Ok(())
}

fn count(n: usize) -> Result<[u8; 2], String> {
    u16::try_from(n).map(u16::to_be_bytes).map_err(|_| format!("{n} items in a table of at most 65,535"))
}

/// The `Code` attribute's bytes with the attributes `attrs`.
pub fn code(c: &Code, attrs: &[Attribute]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(c.bytes.len() + 12);
    out.extend(c.max_stack.to_be_bytes());
    out.extend(c.max_locals.to_be_bytes());
    out.extend((c.bytes.len() as u32).to_be_bytes());
    out.extend(&c.bytes);
    out.extend(count(c.exceptions.len())?);
    out.extend(c.exceptions.iter().flatten().flat_map(|v| v.to_be_bytes()));
    attributes(&mut out, attrs)?;
    Ok(out)
}
