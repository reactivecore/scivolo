// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/Scala2Unpickler.scala
//! The classes and objects of a pickle (dotty's `isClassRoot`, `isModuleClassRoot`,
//! `isModuleRoot`, `adjustIfModuleClass`): the roots and the classes and objects of objects.

use super::super::builtin::synthetic;
use super::super::encoded_names::decode;
use super::super::{Kind, SymbolId, Universe, module_class_name};
use super::flags as f;
use super::format::{CLASSsym, MODULEsym};
use super::symbols::Header;
use super::{Sym, Unpickler};
use crate::tags::{CASE, FINAL, OBJECT};

impl Unpickler {
    /// Enters the class root, the object's class root or the object root; skips another symbol
    /// of the package.
    pub fn root(&mut self, u: &mut Universe, i: usize, h: &Header, pkg: SymbolId) -> Result<Sym, String> {
        if pkg != self.pkg || decode(&h.name) != self.simple {
            return Ok(Sym::Skipped);
        }
        let simple = self.simple.clone();
        self.class_or_object(u, i, h, pkg, &simple)
    }

    /// Enters class, object's class or object entry `i` of header `h` into `owner` as `simple`
    /// (dotty's `adjustIfModuleClass` names the object's class), a class with its type
    /// parameters.
    pub fn class_or_object(
        &mut self,
        u: &mut Universe,
        i: usize,
        h: &Header,
        owner: SymbolId,
        simple: &str,
    ) -> Result<Sym, String> {
        let module = h.flags & f::MODULE != 0;
        let visible = f::visibility_tags(h.flags, h.within.is_some());
        let s = match h.tag {
            CLASSsym if !module => {
                let mut flags = visible;
                flags.extend(f::class_tags(h.flags));
                let class = synthetic(u, owner, simple, Kind::Class, &flags);
                self.enter(u, i, h, class)?;
                self.type_params(u, h, class)?;
                return Ok(Sym::Known(class));
            }
            CLASSsym => {
                let flags = [&[OBJECT, FINAL][..], &visible, &f::case_tag(h.flags)].concat();
                synthetic(u, owner, &module_class_name(simple), Kind::Class, &flags)
            }
            MODULEsym => {
                let implicit = f::member_tags(h.flags & f::IMPLICIT, false);
                let flags = [&[OBJECT, FINAL][..], &visible, &implicit].concat();
                synthetic(u, owner, simple, Kind::Val, &flags)
            }
            _ => return Ok(Sym::Skipped),
        };
        self.enter(u, i, h, s)?;
        Ok(Sym::Known(s))
    }
}

/// Whether class `c` is the class of the companion object of a case class.
pub fn is_case_companion(u: &Universe, c: SymbolId) -> bool {
    let (Some(owner), Some(class)) = (u.symbol(c).owner, u.symbol(c).name.strip_suffix('$')) else {
        return false;
    };
    let decls = &u.symbol(owner).decls;
    decls.iter().any(|&d| {
        let d = u.symbol(d);
        d.name == class && d.kind == Kind::Class && d.flags.has(CASE)
    })
}
