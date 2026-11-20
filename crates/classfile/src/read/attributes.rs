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
    pub scala: Option<Scala>,
}

/// What the Scala compilers' attributes mark a class file as (dotty's
/// `unpickleOrParseInnerClasses`).
#[derive(Clone, Debug, PartialEq)]
pub enum Scala {
    /// A `TASTY` attribute: Scala 3's class file.
    Tasty,
    /// A `Scala` attribute without `TASTY`: a compilation artifact, such as the class `C$` of an
    /// object of Scala 2.
    Artifact,
    /// A `ScalaSig` attribute: Scala 2's pickle, the text of the `bytes` element of the
    /// `ScalaSignature` or `ScalaLongSignature` annotation, one byte per character, still
    /// encoded; empty if the annotation is missing.
    Pickle(Vec<u8>),
}

/// The descriptors of the annotations holding a Scala 2 pickle.
const SIGNATURES: [&str; 2] = ["Lscala/reflect/ScalaSignature;", "Lscala/reflect/ScalaLongSignature;"];

/// Which of the Scala compilers' attributes a class file has, and the text of its pickle
/// annotation's `bytes` element.
#[derive(Default)]
struct Marks {
    tasty: bool,
    scala: bool,
    scala_sig: bool,
    pickle: String,
}

impl Marks {
    fn scala(self) -> Option<Scala> {
        Some(match self {
            Marks { tasty: true, .. } => Scala::Tasty,
            Marks { scala: true, .. } => Scala::Artifact,
            Marks { scala_sig: true, pickle, .. } => Scala::Pickle(pickle.chars().map(|c| c as u8).collect()),
            _ => return None,
        })
    }
}

/// Reads the attributes, skipping those other than `Attributes` holds.
pub fn attributes(b: &mut Bytes, c: &Constants) -> Result<Attributes, String> {
    let mut a = Attributes::default();
    let mut marks = Marks::default();
    for _ in 0..b.u2()? {
        let name = b.u2()?;
        let n = b.u4()? as usize;
        let mut value = Bytes { bytes: b.take(n)?, at: 0 };
        match c.utf8(name).as_deref() {
            Ok("Signature") if n == 2 => a.signature = Some(c.utf8(value.u2()?)?),
            Ok("ConstantValue") if n == 2 => a.constant = Some(c.value(value.u2()?)?),
            Ok("RuntimeVisibleAnnotations" | "RuntimeInvisibleAnnotations") => {
                a.annotations.extend(annotations(&mut value, c, &mut marks.pickle)?)
            }
            Ok("InnerClasses") => a.inner_classes = inner_classes(&mut value, c)?,
            Ok("PermittedSubclasses") => {
                for _ in 0..value.u2()? {
                    a.permitted.push(c.class(value.u2()?)?.ok_or("a permitted subclass without a name")?);
                }
            }
            Ok("AnnotationDefault") => a.annotation_default = true,
            Ok("MethodParameters") => a.param_names = method_parameters(&mut value, c)?,
            Ok("TASTY") => marks.tasty = true,
            Ok("Scala") => marks.scala = true,
            Ok("ScalaSig") => marks.scala_sig = true,
            _ => {}
        }
    }
    a.scala = marks.scala();
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

/// An element value an `Annotation` keeps, or a string or the strings of an array of them.
enum Kept {
    Enum(String),
    Class(String),
    Strings(String),
}

/// The annotations of an annotations attribute's bytes; the text of a Scala 2 pickle annotation's
/// `bytes` element appended to `pickle`.
fn annotations(b: &mut Bytes, c: &Constants, pickle: &mut String) -> Result<Vec<Annotation>, String> {
    (0..b.u2()?).map(|_| annotation(b, c, pickle)).collect()
}

fn annotation(b: &mut Bytes, c: &Constants, pickle: &mut String) -> Result<Annotation, String> {
    let descriptor = c.utf8(b.u2()?)?;
    let (mut enum_elements, mut class_elements) = (Vec::new(), Vec::new());
    for _ in 0..b.u2()? {
        let name = c.utf8(b.u2()?)?;
        match element_value(b, c)? {
            Some(Kept::Enum(constant)) => enum_elements.push((name, constant)),
            Some(Kept::Class(class)) => class_elements.push((name, class)),
            Some(Kept::Strings(s)) if name == "bytes" && SIGNATURES.contains(&descriptor.as_str()) => {
                pickle.push_str(&s)
            }
            _ => {}
        }
    }
    Ok(Annotation { descriptor, enum_elements, class_elements })
}

/// Skips an element value; the value if it is an enum constant, a class literal, a string or an
/// array of strings (concatenated).
fn element_value(b: &mut Bytes, c: &Constants) -> Result<Option<Kept>, String> {
    let tag = b.take(1)?[0];
    match tag {
        b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' => _ = b.u2()?,
        b's' => return c.utf8(b.u2()?).map(|s| Some(Kept::Strings(s))),
        b'c' => return c.utf8(b.u2()?).map(|d| Some(Kept::Class(d))),
        b'e' => {
            b.u2()?;
            return c.utf8(b.u2()?).map(|n| Some(Kept::Enum(n)));
        }
        b'@' => _ = annotation(b, c, &mut String::new())?,
        b'[' => {
            let mut strings = Some(String::new());
            for _ in 0..b.u2()? {
                match (element_value(b, c)?, strings.as_mut()) {
                    (Some(Kept::Strings(s)), Some(all)) => all.push_str(&s),
                    _ => strings = None,
                }
            }
            return Ok(strings.map(Kept::Strings));
        }
        _ => return Err(format!("annotation element tag {tag}")),
    }
    Ok(None)
}
