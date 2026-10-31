// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymUtils.scala
//   compiler/src/dotty/tools/dotc/transform/Dependencies.scala
//! dotty's `markFree` and the enclosures it walks.

use super::Collect;
use crate::phases::local_free::{is_method, is_term, within};
use crate::symbols::has;
use tasty::tags::LAZY;
use tasty::{Kind, SymbolId};

impl Collect<'_, '_> {
    /// The closest method or class `s` is or is in, a class's constructor for a field or its
    /// getter (dotty's `enclosingMethodOrClass`).
    fn method_or_class(&self, mut s: SymbolId) -> Option<SymbolId> {
        let l = self.l;
        loop {
            let sym = l.u.symbol(s);
            let owner = sym.owner;
            let field = owner.is_some_and(|o| l.u.is_class(o)) && !has(l.u, s, LAZY);
            match sym.kind {
                Kind::Package => return None,
                Kind::Class => return Some(s),
                Kind::Def if l.u.is_constructor(s) || is_method(l, Some(s)) => return Some(s),
                Kind::Def | Kind::Val if field => return owner.and_then(|o| self.ctors.get(&o).copied()),
                Kind::Val if has(l.u, s, LAZY) => return Some(s),
                _ => s = owner?,
            }
        }
    }

    /// The closest method or class properly enclosing `s` (dotty's `enclosure`).
    fn enclosure(&self, s: SymbolId) -> Option<SymbolId> {
        self.method_or_class(self.l.u.symbol(s).owner?)
    }

    fn is_real_class(&self, s: Option<SymbolId>) -> bool {
        s.is_some_and(|s| self.l.u.is_class(s) && !self.l.u.is_trait(s))
    }

    fn nested_in_constructor(&self, s: SymbolId) -> bool {
        self.l.u.is_constructor(s)
            || is_term(self.l, s) && self.enclosure(s).is_some_and(|e| self.nested_in_constructor(e))
    }

    /// Marks `v` free in `encl` unless defined there or free in a class between them; the
    /// largest class between them, else `encl` if a class (dotty's `markFree`).
    pub fn mark_free(&mut self, v: SymbolId, encl: Option<SymbolId>) -> Option<SymbolId> {
        let encl = encl?;
        if self.enclosure(v) == Some(encl) {
            return None;
        }
        let outer = match self.l.u.is_constructor(encl) {
            true => self.l.u.symbol(encl).owner.and_then(|c| self.enclosure(c)),
            false => self.enclosure(encl),
        };
        let intermediate = self.mark_free(v, outer);
        let l = self.l;
        let local_class = l.u.is_class(encl) && l.u.symbol(encl).owner.is_some_and(|o| is_term(l, o));
        if let Some(i) = intermediate.filter(|_| local_class) {
            let owner = self.logic_owners.entry(encl).or_insert(i);
            *owner = if within(l, i, *owner) { i } else { *owner };
        }
        if (!self.is_real_class(intermediate) || self.nested_in_constructor(encl)) && !self.l.u.is_trait(encl)
        {
            match self.free.iter_mut().find(|(o, _)| *o == encl) {
                Some((_, vs)) if vs.contains(&v) => {}
                Some((_, vs)) => {
                    vs.push(v);
                    self.changed = true;
                }
                None => {
                    self.free.push((encl, vec![v]));
                    self.order.push((encl, false));
                    self.changed = true;
                }
            }
        }
        let class = |s: Option<SymbolId>| s.is_some_and(|s| self.l.u.is_class(s));
        [intermediate, Some(encl)]
            .into_iter()
            .find(|&s| self.is_real_class(s))
            .or_else(|| [intermediate, Some(encl)].into_iter().find(|&s| class(s)))
            .flatten()
    }
}
