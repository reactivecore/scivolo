// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//   compiler/src/dotty/tools/dotc/core/TypeEval.scala
//! `NamedTuple.From[T]` folded (dotty's `isNamedTuple_From` and `fieldsOf`): the fields of a
//! case class, a named tuple itself, or a tuple's elements named `_1`, `_2`, ….

use super::super::Cmp;
use crate::tags::{CASE, CASEaccessor};
use crate::types::{Type, TypeId};
use crate::{Kind, SymbolId};

/// How many elements a tuple folded to a named tuple may have.
const MAX_ELEMENTS: usize = 256;

impl Cmp<'_> {
    /// Whether `tycon` is `scala.NamedTuple.From`.
    pub fn is_named_tuple_from(&mut self, tycon: TypeId) -> bool {
        let Some(s) = self.named(tycon).and_then(|n| n.sym) else { return false };
        let owner = self.u.symbol(s).owner;
        self.u.symbol(s).name == "From" && owner.is_some_and(|o| self.u.full_name(o) == "scala.NamedTuple$")
    }

    /// `From[arg]` with `from` the reference to `From`, folded if `arg` is known enough.
    pub fn fields_of(&mut self, from: TypeId, arg: TypeId) -> Option<TypeId> {
        let (prefix, s) = match *self.u.types().get(from) {
            Type::Sym(p, s) => (p, s),
            _ => return None,
        };
        let owner = self.u.symbol(s).owner?;
        let nt = self
            .u
            .decl(owner, "NamedTuple")
            .ok()?
            .into_iter()
            .find(|&d| self.u.symbol(d).kind == Kind::Type)?;
        let nt = self.u.intern_type(Type::Sym(prefix, nt));
        let (labels, fields): (Vec<String>, Vec<TypeId>) = match self.u.class_symbol(arg) {
            Some(c) if self.u.symbol(c).flags.has(CASE) && self.is_concrete(arg, 32) => {
                let accessors: Vec<SymbolId> = self.u.symbol(c).decls.clone();
                let accessors = accessors
                    .into_iter()
                    .filter(|&d| self.u.symbol(d).kind.is_term() && self.u.symbol(d).flags.has(CASEaccessor));
                let accessors: Vec<SymbolId> = accessors.collect();
                let labels = accessors.iter().map(|&a| self.u.symbol(a).name.clone()).collect();
                (labels, accessors.into_iter().map(|a| self.u.member_info(arg, a)).collect())
            }
            _ => {
                let widened = self.u.widen(arg);
                let d = self.u.dealias(widened);
                if let Type::Applied(tycon, _) = *self.u.types().get(d)
                    && self.named(tycon).and_then(|n| n.sym) == self.named(nt).and_then(|n| n.sym)
                {
                    return Some(d);
                }
                let elems = self.tuple_elems(d)?;
                ((1..=elems.len()).map(|i| format!("_{i}")).collect(), elems)
            }
        };
        let labels: Vec<TypeId> = labels.into_iter().map(|l| self.u.intern_type(Type::Str(l))).collect();
        let (labels, fields) = (self.pairs(&labels)?, self.pairs(&fields)?);
        Some(self.u.intern_type(Type::Applied(nt, vec![labels, fields])))
    }

    /// The element types of tuple type `t`: a `TupleN` or a `*:` chain ending in `EmptyTuple`.
    fn tuple_elems(&mut self, t: TypeId) -> Option<Vec<TypeId>> {
        let (mut out, mut t) = (Vec::new(), t);
        for _ in 0..MAX_ELEMENTS {
            let d = self.u.dealias(t);
            let Type::Applied(tycon, args) = self.u.types().get(d).clone() else {
                let class = self.u.class_symbol(d)?;
                return (self.u.full_name(class) == "scala.EmptyTuple$").then_some(out);
            };
            let c = self.u.class_symbol(tycon)?;
            if let Some(pairs) = self.nested_pairs(c, &args) {
                t = pairs;
                continue;
            }
            let [head, tail] = args[..] else { return None };
            if self.u.full_name(c) != "scala.*:" {
                return None;
            }
            out.push(head);
            t = tail;
        }
        None
    }

    /// `elems` as `A *: B *: … *: EmptyTuple`.
    fn pairs(&mut self, elems: &[TypeId]) -> Option<TypeId> {
        let scala = self.u.package("scala")?;
        let found = |s: &mut Self, name: &str, kind: Kind| {
            s.u.decl(scala, name).ok()?.into_iter().find(|&d| s.u.symbol(d).kind == kind)
        };
        let (pair, empty) = (found(self, "*:", Kind::Class)?, found(self, "EmptyTuple", Kind::Type)?);
        let pair = self.u.intern_type(Type::Sym(None, pair));
        let mut t = self.u.intern_type(Type::Sym(None, empty));
        for &e in elems.iter().rev() {
            t = self.u.intern_type(Type::Applied(pair, vec![e, t]));
        }
        Some(t)
    }
}
