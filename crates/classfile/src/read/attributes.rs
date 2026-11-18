//! The attributes of classes and members read: `Signature` (JVMS §4.7.9), `ConstantValue`, the
//! annotations (§4.7.16), `InnerClasses` (§4.7.6), `PermittedSubclasses` (§4.7.31),
//! `MethodParameters` (§4.7.24), whether there is an `AnnotationDefault` (§4.7.22) and the Scala
//! compilers' markers.

use super::{Bytes, ConstantValue, Constants};
use crate::{ACC_SYNTHETIC, InnerClass};

/// The attributes of a class or member read.
#[derive(Default)]
pub struct Attributes {
    pub signature: Option<String>,
    pub constant: Option<ConstantValue>,
    pub annotations: Vec<Annotation>,
    pub inner_classes: Vec<InnerClass>,
    pub permitted: Vec<String>,
    pub annotation_default: bool,
    pub param_names: Vec<Option<String>>,
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
            Ok("PermittedSubclasses") => {
                for _ in 0..value.u2()? {
                    a.permitted.push(c.class(value.u2()?)?.ok_or("a permitted subclass without a name")?);
                }
            }
            Ok("AnnotationDefault") => a.annotation_default = true,
            Ok("MethodParameters") => a.param_names = method_parameters(&mut value, c)?,
            Ok("Scala" | "ScalaSig" | "TASTY") => a.scala = true,
            _ => {}
        }
    }
    Ok(a)
}

/// The names of a `MethodParameters` attribute by position, none for a nameless or synthetic
/// parameter (dotty's `namedParams`).
fn method_parameters(b: &mut Bytes, c: &Constants) -> Result<Vec<Option<String>>, String> {
    (0..b.take(1)?[0])
        .map(|_| {
            let (name, flags) = (b.u2()?, b.u2()?);
            let named = name != 0 && flags & ACC_SYNTHETIC == 0;
            named.then(|| c.utf8(name)).transpose()
        })
        .collect()
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

/// An annotation of a class: its type's descriptor, the elements whose values are enum
/// constants, each its name and the constant's name, and those whose values are class literals,
/// each its name and the class's return descriptor.
#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub descriptor: String,
    pub enum_elements: Vec<(String, String)>,
    pub class_elements: Vec<(String, String)>,
}

/// An element value an `Annotation` keeps.
enum Kept {
    Enum(String),
    Class(String),
}

/// The annotations of an annotations attribute's bytes.
fn annotations(b: &mut Bytes, c: &Constants) -> Result<Vec<Annotation>, String> {
    (0..b.u2()?).map(|_| annotation(b, c)).collect()
}

fn annotation(b: &mut Bytes, c: &Constants) -> Result<Annotation, String> {
    let descriptor = c.utf8(b.u2()?)?;
    let (mut enum_elements, mut class_elements) = (Vec::new(), Vec::new());
    for _ in 0..b.u2()? {
        let name = c.utf8(b.u2()?)?;
        match element_value(b, c)? {
            Some(Kept::Enum(constant)) => enum_elements.push((name, constant)),
            Some(Kept::Class(class)) => class_elements.push((name, class)),
            None => {}
        }
    }
    Ok(Annotation { descriptor, enum_elements, class_elements })
}

/// Skips an element value; the value if it is an enum constant or a class literal.
fn element_value(b: &mut Bytes, c: &Constants) -> Result<Option<Kept>, String> {
    let tag = b.take(1)?[0];
    match tag {
        b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' | b's' => _ = b.u2()?,
        b'c' => return c.utf8(b.u2()?).map(|d| Some(Kept::Class(d))),
        b'e' => {
            b.u2()?;
            return c.utf8(b.u2()?).map(|n| Some(Kept::Enum(n)));
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
