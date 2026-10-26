// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Selections on small generic tuples (dotty's `trySmallGenericTuple`).

use super::Typer;
use crate::tpd::TNodeId;
use tasty::{SymbolId, Type, TypeId};

/// The largest arity of a `TupleN` class.
pub const MAX_TUPLE_ARITY: usize = 22;

impl Typer<'_> {
    /// `qual.$asInstanceOf$[TupleN[A, B, …]]` when `qual` is an `A *: B *: … *: EmptyTuple` of at
    /// most 22 elements, none otherwise.
    pub fn small_generic_tuple(&mut self, qual: TNodeId) -> Option<TNodeId> {
        let tpe = self.tpd.tpe(qual);
        let widened = self.u.widen(tpe);
        let elems = self.tuple_elements(widened)?;
        if elems.is_empty() || elems.len() > MAX_TUPLE_ARITY {
            return None;
        }
        let class = self.resolve_full_name(&format!("scala.Tuple{}", elems.len()), true)?;
        let class = self.u.intern_type(Type::Sym(None, class));
        let to = self.u.intern_type(Type::Applied(class, elems));
        Some(self.type_cast(qual, to))
    }

    /// The element types of `t` if it is a chain `A *: B *: … *: T`, `T` `EmptyTuple` or a `TupleN`.
    pub fn tuple_elements(&mut self, t: TypeId) -> Option<Vec<TypeId>> {
        self.tuple_elements_within(t, MAX_TUPLE_ARITY + 1)
    }

    /// `tuple_elements` of a chain of at most `pairs` `*:`.
    pub fn tuple_elements_within(&mut self, t: TypeId, pairs: usize) -> Option<Vec<TypeId>> {
        let pair = self.resolve_full_name("scala.*:", true)?;
        let mut elems = Vec::new();
        let mut t = t;
        for _ in 0..pairs {
            let d = self.u.dealias(t);
            if let Type::Applied(f, args) = self.u.types().get(d).clone()
                && let [head, tail] = args[..]
                && self.u.class_symbol(f) == Some(pair)
            {
                elems.push(head);
                t = tail;
                continue;
            }
            if let Type::Applied(f, args) = self.u.types().get(d).clone()
                && !elems.is_empty()
                && self.u.class_symbol(f).is_some_and(|c| self.is_tuple_class(c, args.len()))
            {
                elems.extend(args);
                return Some(elems);
            }
            return self.is_empty_tuple(d).then_some(elems);
        }
        None
    }

    /// Whether `c` is class `scala.TupleN` of arity `n`.
    fn is_tuple_class(&self, c: SymbolId, n: usize) -> bool {
        self.u.full_name(c) == format!("scala.Tuple{n}")
    }

    /// Whether `t` is `EmptyTuple`, the alias or the object's type.
    fn is_empty_tuple(&mut self, t: TypeId) -> bool {
        let mut t = t;
        for _ in 0..4 {
            if let Type::Sym(_, s) = *self.u.types().get(t)
                && self.u.symbol(s).kind == tasty::Kind::Type
            {
                self.complete(s);
                let info = self.u.info(s);
                if let Type::Alias(a) = *self.u.types().get(info) {
                    t = a;
                    continue;
                }
            }
            let class = match *self.u.types().get(t) {
                Type::Sym(_, s) if self.u.symbol(s).flags.has(tasty::tags::OBJECT) => {
                    self.u.module_class(s).or_else(|| self.u.is_class(s).then_some(s))
                }
                _ => {
                    let w = self.u.widen(t);
                    self.u.class_symbol(w)
                }
            };
            return class.is_some_and(|c| {
                self.u.symbol(c).name == "EmptyTuple$" && self.u.full_name(c).starts_with("scala.")
            });
        }
        false
    }
}
