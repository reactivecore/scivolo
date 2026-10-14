//! The primitive value classes' objects and their `box` and `unbox` methods, each resolved on
//! first use.

use super::{member, package};
use crate::Lower;
use tasty::{Kind, SymbolId, Universe};

/// A primitive value class: its erased letter, its object and that object's `box` and `unbox`
/// methods.
#[derive(Clone, Copy)]
pub struct Primitive {
    pub letter: char,
    pub module: SymbolId,
    pub box_method: SymbolId,
    pub unbox_method: SymbolId,
}

/// The primitive value classes of package `scala` and their erased letters.
pub const PRIMITIVES: [(&str, char); 8] = [
    ("Boolean", 'z'),
    ("Char", 'c'),
    ("Byte", 'b'),
    ("Short", 's'),
    ("Int", 'i'),
    ("Long", 'j'),
    ("Float", 'f'),
    ("Double", 'd'),
];

/// The box class of the primitive of erased letter `c`.
pub fn boxed_class(c: char) -> Option<&'static str> {
    Some(match c {
        'z' => "java.lang.Boolean",
        'c' => "java.lang.Character",
        'b' => "java.lang.Byte",
        's' => "java.lang.Short",
        'i' => "java.lang.Integer",
        'j' => "java.lang.Long",
        'f' => "java.lang.Float",
        'd' => "java.lang.Double",
        _ => return None,
    })
}

/// The binary name of the primitive value class of erased letter `letter`.
pub fn primitive_class_name(letter: char) -> Option<String> {
    PRIMITIVES.iter().find(|&&(_, l)| l == letter).map(|(name, _)| format!("scala.{name}"))
}

impl Lower<'_> {
    /// The primitive of erased letter `letter`; `None` for another letter.
    pub fn primitive(&mut self, letter: char) -> Option<Primitive> {
        let i = PRIMITIVES.iter().position(|&(_, l)| l == letter)?;
        if let Some(p) = self.defs.primitives[i] {
            return Some(p);
        }
        let p = resolve(self.u, i).unwrap_or_else(|e| panic!("{e}"));
        self.defs.primitives[i] = Some(p);
        Some(p)
    }

    /// The primitive whose object's class declares method `m`.
    fn declaring_primitive(&mut self, m: SymbolId) -> Option<Primitive> {
        let class = self.u.symbol(self.u.symbol(m).owner?);
        let name = class.name.strip_suffix('$')?;
        let &(_, letter) = PRIMITIVES.iter().find(|(n, _)| *n == name)?;
        if class.owner != self.u.package("scala") {
            return None;
        }
        self.primitive(letter)
    }

    pub fn is_box(&mut self, m: SymbolId) -> bool {
        self.u.symbol(m).name == "box" && self.declaring_primitive(m).is_some_and(|p| p.box_method == m)
    }

    /// The primitive whose `unbox` method `m` is.
    pub fn unboxed_by(&mut self, m: SymbolId) -> Option<Primitive> {
        if self.u.symbol(m).name != "unbox" {
            return None;
        }
        self.declaring_primitive(m).filter(|p| p.unbox_method == m)
    }
}

fn resolve(u: &mut Universe, i: usize) -> Result<Primitive, String> {
    let (name, letter) = PRIMITIVES[i];
    let scala = package(u, "scala")?;
    let module = member(u, scala, name, Kind::Val)?;
    let class = u.module_class(module).ok_or_else(|| format!("scala.{name}'s object missing"))?;
    let box_method = member(u, class, "box", Kind::Def)?;
    let unbox_method = member(u, class, "unbox", Kind::Def)?;
    Ok(Primitive { letter, module, box_method, unbox_method })
}
