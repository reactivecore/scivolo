// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/core/TypeUtils.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Selections on small generic tuples (dotty's `trySmallGenericTuple`), their element types, and
//! the generic tuple types of elements.

use super::Typer;
use crate::tpd::TNodeId;
use tasty::{SymbolId, Type, TypeId, Universe};

pub use tasty::defn::MAX_TUPLE_ARITY;

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
            if let Some((f, &[head, tail])) = self.u.applied(d)
                && self.u.class_symbol(f) == Some(pair)
            {
                elems.push(head);
                t = tail;
                continue;
            }
            if let Some((f, args)) = self.u.applied(d).map(|(f, args)| (f, args.to_vec()))
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

    /// Normalizes a chain `A *: B *: … *: EmptyTuple` of at most 22 elements to `TupleN[A, B, …]`;
    /// returns any other `t` unchanged (dotty's `normalizedTupleType`).
    pub fn normalized_tuple_type(&mut self, t: TypeId) -> TypeId {
        let Some(elems) = self.tuple_elements(t).filter(|e| (1..=MAX_TUPLE_ARITY).contains(&e.len())) else {
            return t;
        };
        match self.resolve_full_name(&format!("scala.Tuple{}", elems.len()), true) {
            Some(class) => crate::defn::applied(self.u, class, elems),
            None => t,
        }
    }

    /// Builds the generic tuple type `A *: B *: EmptyTuple` of `elems` (dotty's
    /// `TypeOps.nestedPairs`).
    pub fn nested_pairs(&mut self, elems: &[TypeId]) -> Result<TypeId, &'static str> {
        const MISSING: &str = "Mirror without its library classes";
        let pair = self.resolve_full_name("scala.*:", true).ok_or(MISSING)?;
        let empty = self.resolve_full_name("scala.EmptyTuple", false).ok_or(MISSING)?;
        let pair = crate::defn::type_ref(self.u, pair);
        let mut t = self.u.intern_type(Type::Sym(None, empty));
        for &e in elems.iter().rev() {
            t = self.u.intern_type(Type::Applied(pair, vec![e, t]));
        }
        Ok(t)
    }

    /// Whether `c` is class `scala.TupleN` of arity `n`.
    fn is_tuple_class(&self, c: SymbolId, n: usize) -> bool {
        self.u.full_name(c) == format!("scala.Tuple{n}")
    }

    /// Whether `t` is `EmptyTuple`, the alias or the object's type.
    fn is_empty_tuple(&mut self, t: TypeId) -> bool {
        let mut t = t;
        for _ in 0..4 {
            if let Some(s) = self.u.sym_of(t)
                && self.u.symbol(s).kind == tasty::Kind::Type
            {
                self.complete(s);
                let info = self.u.info(s);
                if let Some(a) = self.u.type_alias(info) {
                    t = a;
                    continue;
                }
            }
            let class = match self.u.sym_of(t) {
                Some(s) if self.u.symbol(s).flags.has(tasty::tags::OBJECT) => {
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

/// Lists the element types of a tuple written with its class, such as `Tuple2[Int, String]` or
/// an alias of it; `None` for another type.
pub fn tuple_class_args(u: &mut Universe, t: TypeId) -> Option<Vec<TypeId>> {
    let d = u.dealias(t);
    let (f, args) = u.applied(d).map(|(f, args)| (f, args.to_vec()))?;
    let c = u.class_symbol(f)?;
    (u.full_name(c) == format!("scala.Tuple{}", args.len())).then_some(args)
}
