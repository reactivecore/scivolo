//! The JDK's packages and classes of `java.*` and `javax.*` the library lacks, entered from the
//! JDK's class files when a universe has a JDK (ADR 0107).

mod annotation;
mod class;
mod classpath;
mod constant;
mod inner;
mod javalib;
mod signatures;
mod visibility;

pub use annotation::Retention;
pub use classpath::JavaClasses;

use super::{Kind, SymbolId, Universe};
use crate::Erased;
use classfile::{ACC_MODULE, ACC_PUBLIC};
use names::nme;

impl Universe {
    /// Makes the JDK's packages and classes the library lacks known from `jdk`'s class files.
    pub fn set_jdk(&mut self, jdk: classfile::Jdk) {
        self.jdk = Some(jdk);
    }

    /// Package or public class `name` of package `pkg` (an object `C$` stands for its class `C`)
    /// entered from the JDK, if it has it in a `java.*` or `javax.*` package and the universe has
    /// a JDK and the package lacks the class and its object; nested classes are not entered.
    pub fn jdk_decl(&mut self, pkg: SymbolId, name: crate::NameId) -> Vec<SymbolId> {
        if self.jdk.is_none() {
            return Vec::new();
        }
        let name = self.name(name).to_owned();
        let name = name.as_str();
        let outer = self.full_name(pkg).replace('.', "/");
        let full = if outer.is_empty() { name.to_owned() } else { format!("{outer}/{name}") };
        let java = ["java", "javax"].iter().any(|p| full == *p || full.starts_with(&format!("{p}/")));
        if !java || name.contains(['.', '/']) {
            return Vec::new();
        }
        if self.jdk.as_mut().is_some_and(|jdk| jdk.has_package(&full)) {
            return vec![self.enter_package(&full.replace('/', "."))];
        }
        let class = name.strip_suffix('$').unwrap_or(name);
        let names = [class.to_owned(), super::module_class_name(class)];
        let defined = self.symbol(pkg).decls.iter().any(|&d| names.contains(&self.symbol(d).name));
        if outer.is_empty() || class.contains('$') || defined {
            return Vec::new();
        }
        let jdk = self.jdk.as_mut().expect("a JDK");
        let Some(info) = jdk.class(&format!("{outer}/{class}")).cloned() else { return Vec::new() };
        if info.access & ACC_PUBLIC == 0 || info.access & ACC_MODULE != 0 {
            return Vec::new();
        }
        let entered = class::enter(self, pkg, class, &info, info.access).unwrap_or_default();
        entered.into_iter().filter(|&d| self.symbol(d).name == name).collect()
    }

    /// Whether `s` is a class, object or object's class entered from a JDK class file.
    pub fn is_from_classfile(&self, s: SymbolId) -> bool {
        self.classfile_classes.contains(&s)
    }

    /// The retention of `c` if it is an annotation class entered from a JDK class file.
    pub fn classfile_retention(&self, c: SymbolId) -> Option<Retention> {
        self.classfile_annotations.get(&c).copied()
    }

    /// Whether `s` is a concrete member entered from a JDK class file.
    pub fn is_concrete_classfile_member(&self, s: SymbolId) -> bool {
        self.classfile_descriptors.contains_key(&s) && !self.is_deferred(s)
    }

    /// The erasure of method `def` entered from a JDK class file, its descriptor's; a
    /// constructor's result its class.
    pub fn classfile_erasure(&self, def: SymbolId) -> Option<(Vec<Erased>, Erased)> {
        let descriptor = self.classfile_descriptors.get(&def)?;
        let (params, result) = descriptor.strip_prefix('(')?.split_once(')')?;
        let mut rest = params;
        let mut erased = Vec::new();
        while !rest.is_empty() {
            let (e, r) = erase(rest)?;
            erased.push(e);
            rest = r;
        }
        let owner = self.symbol(def).owner?;
        let result = match self.symbol(def).name == nme::CONSTRUCTOR && self.symbol(owner).kind == Kind::Class
        {
            true => Erased::Class(self.full_name(owner)),
            false => erase(result)?.0,
        };
        Some((erased, result))
    }
}

/// The erasure of the field descriptor `descriptor` starts with, and the rest.
fn erase(descriptor: &str) -> Option<(Erased, &str)> {
    let (tag, rest) = descriptor.split_at_checked(1)?;
    let prim = |c| Some((Erased::Prim(c), rest));
    match tag {
        "Z" => prim('z'),
        "C" => prim('c'),
        "B" => prim('b'),
        "S" => prim('s'),
        "I" => prim('i'),
        "J" => prim('j'),
        "F" => prim('f'),
        "D" => prim('d'),
        "V" => Some((Erased::Unit, rest)),
        "[" => erase(rest).map(|(e, r)| (Erased::Array(Box::new(e)), r)),
        "L" => rest.split_once(';').map(|(name, r)| (Erased::Class(name.replace('/', ".")), r)),
        _ => None,
    }
}
