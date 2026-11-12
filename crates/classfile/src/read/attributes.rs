//! The attributes of classes and members read: `Signature` (JVMS §4.7.9), `ConstantValue`, the
//! annotations (§4.7.16), `InnerClasses` (§4.7.6) and the Scala compilers' markers.

use super::{Bytes, ConstantValue, Constants};
use crate::InnerClass;

/// The attributes of a class or member read.
#[derive(Default)]
pub struct Attributes {
    pub signature: Option<String>,
    pub constant: Option<ConstantValue>,
    pub annotations: Vec<Annotation>,
    pub inner_classes: Vec<InnerClass>,
    pub scala: bool,
}

/// Reads the attributes, skipping those other than `Attributes` holds.
pub fn attributes(b: &mut Bytes, c: &Constants) -> Result<Attributes, String> {
    let mut a = Attributes::default();
    for _ in 0..b.u2()? {
        let name = b.u2()?;
        let n = b.u4()? as usize;
        let mut value = Bytes { bytes: b.take(n)?, at: 0 };
        match c.utf8(name).as_deref() {
            Ok("Signature") if n == 2 => a.signature = Some(c.utf8(value.u2()?)?),
            Ok("ConstantValue") if n == 2 => a.constant = Some(c.value(value.u2()?)?),
            Ok("RuntimeVisibleAnnotations" | "RuntimeInvisibleAnnotations") => {
                a.annotations.extend(annotations(&mut value, c)?)
            }
            Ok("InnerClasses") => a.inner_classes = inner_classes(&mut value, c)?,
            Ok("Scala" | "ScalaSig" | "TASTY") => a.scala = true,
            _ => {}
        }
    }
    Ok(a)
}

fn inner_classes(b: &mut Bytes, c: &Constants) -> Result<Vec<InnerClass>, String> {
    (0..b.u2()?)
        .map(|_| {
            let inner = c.class(b.u2()?)?.ok_or("an inner class without a name")?;
            let outer = c.class(b.u2()?)?;
            let name = match b.u2()? {
                0 => None,
                i => Some(c.utf8(i)?),
            };
            Ok(InnerClass { inner, outer, name, flags: b.u2()? })
        })
        .collect()
}

/// An annotation of a class: its type's descriptor and the elements whose values are enum
/// constants, each its name and the constant's name.
#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub descriptor: String,
    pub enum_elements: Vec<(String, String)>,
}

/// The annotations of an annotations attribute's bytes.
fn annotations(b: &mut Bytes, c: &Constants) -> Result<Vec<Annotation>, String> {
    (0..b.u2()?).map(|_| annotation(b, c)).collect()
}

fn annotation(b: &mut Bytes, c: &Constants) -> Result<Annotation, String> {
    let descriptor = c.utf8(b.u2()?)?;
    let mut enum_elements = Vec::new();
    for _ in 0..b.u2()? {
        let name = c.utf8(b.u2()?)?;
        if let Some(constant) = element_value(b, c)? {
            enum_elements.push((name, constant));
        }
    }
    Ok(Annotation { descriptor, enum_elements })
}

/// Skips an element value; the constant's name if it is an enum constant.
fn element_value(b: &mut Bytes, c: &Constants) -> Result<Option<String>, String> {
    let tag = b.take(1)?[0];
    match tag {
        b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' | b's' | b'c' => _ = b.u2()?,
        b'e' => {
            b.u2()?;
            return c.utf8(b.u2()?).map(Some);
        }
        b'@' => _ = annotation(b, c)?,
        b'[' => {
            for _ in 0..b.u2()? {
                element_value(b, c)?;
            }
        }
        _ => return Err(format!("annotation element tag {tag}")),
    }
    Ok(None)
}
