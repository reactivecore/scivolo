// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//   compiler/src/dotty/tools/dotc/core/TypeUtils.scala
//! The class a synthesized `ClassTag[T]` names: whether `T` erases alike in every instance and
//! the class it erases to.

use super::super::Typer;
use super::super::small_tuples::MAX_TUPLE_ARITY;
use tasty::{Erased, Kind, SymbolId, Type, TypeId};

const EMPTY_TUPLE: &str = "scala.Tuple$package$.EmptyTuple$";

impl Typer<'_> {
    /// Whether every instance of `t` erases alike (dotty's `hasStableErasure`).
    pub fn has_stable_erasure(&mut self, t: TypeId) -> bool {
        let t = self.translucent(t);
        match self.u.types().get(t).clone() {
            Type::And(a, b) | Type::Or(a, b, _) => self.has_stable_erasure(a) && self.has_stable_erasure(b),
            Type::Applied(inner, _) | Type::Refined(inner, ..) | Type::Annotated(inner, _) => {
                self.has_stable_erasure(inner)
            }
            Type::Constant(..) | Type::Str(_) => true,
            Type::Sym(..) | Type::TypeRef(..) | Type::TermRef(..) => match self.referenced(t) {
                Some(s) if self.u.symbol(s).kind == Kind::Class => true,
                Some(s) if self.u.symbol(s).kind.is_term() => {
                    let widened = self.u.widen(t);
                    widened != t && self.has_stable_erasure(widened)
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Whether `t` is `Nothing` or `Null`, an intersection with such a side, or a union of such
    /// sides.
    pub fn is_bottom(&mut self, t: TypeId) -> bool {
        let t = self.u.dealias(t);
        match *self.u.types().get(t) {
            Type::And(a, b) => self.is_bottom(a) || self.is_bottom(b),
            Type::Or(a, b, _) => self.is_bottom(a) && self.is_bottom(b),
            _ => {
                let b = self.u.builtins();
                self.u.class_symbol(t).is_some_and(|c| c == b.nothing || c == b.null)
            }
        }
    }

    /// A reference to the class `t` erases to, `t` seen through opaque types and with generic
    /// tuples normalized: a value class itself, else the class among those of the sides of `t`,
    /// their base classes and `Object` that erases as `t` does.
    pub fn erased_class_type(&mut self, t: TypeId) -> Option<TypeId> {
        let t = self.translucent(t);
        let t = self.normalized_tuple_type(t);
        let mut classes = Vec::new();
        self.side_classes(t, &mut classes);
        if let [c] = classes[..]
            && self.u.underlying(c).is_some()
        {
            return Some(crate::defn::type_ref(self.u, c));
        }
        let tuple = |u: &mut tasty::Universe, c| matches!(u.full_name(c).as_str(), "scala.*:" | EMPTY_TUPLE);
        if classes.is_empty() || classes.iter().any(|&c| tuple(self.u, c)) {
            return None;
        }
        let erased = self.u.erase(t);
        let mut bases: Vec<SymbolId> = classes.iter().flat_map(|&c| self.u.linearization(c)).collect();
        bases.extend(self.resolve_full_name("java.lang.Object", true));
        let found = bases.into_iter().find(|&c| {
            let class = crate::defn::type_ref(self.u, c);
            self.u.underlying(c).is_none()
                && match &erased {
                    Erased::Class(name) => self.u.binary_name(c) == *name,
                    _ => self.u.erase(class) == erased,
                }
        })?;
        Some(crate::defn::type_ref(self.u, found))
    }

    /// Adds the classes of the sides of intersection or union `t`, or of `t`, to `out`.
    fn side_classes(&mut self, t: TypeId, out: &mut Vec<SymbolId>) {
        let t = self.translucent(t);
        match *self.u.types().get(t) {
            Type::And(a, b) | Type::Or(a, b, _) => {
                self.side_classes(a, out);
                self.side_classes(b, out);
            }
            _ => {
                let widened = self.u.widen(t);
                let widened = self.u.dealias(widened);
                out.extend(self.u.class_symbol(widened));
            }
        }
    }

    /// `TupleN[A, B, …]` for a chain `A *: B *: … *: EmptyTuple` of at most 22 elements, else `t`
    /// (dotty's `normalizedTupleType`).
    fn normalized_tuple_type(&mut self, t: TypeId) -> TypeId {
        let Some(elems) = self.tuple_elements(t).filter(|e| (1..=MAX_TUPLE_ARITY).contains(&e.len())) else {
            return t;
        };
        match self.resolve_full_name(&format!("scala.Tuple{}", elems.len()), true) {
            Some(class) => crate::defn::applied(self.u, class, elems),
            None => t,
        }
    }

    /// `t` dealiased, its match types reduced and its opaque types replaced by their right-hand
    /// sides, as far as they go (dotty's `translucentSuperType`).
    fn translucent(&mut self, t: TypeId) -> TypeId {
        let mut t = self.u.dealias(t);
        for _ in 0..16 {
            let next = match *self.u.types().get(t) {
                Type::Match { .. } => self.u.try_normalize(t),
                Type::Alias(a) => Some(a),
                _ => self.opaque_expansion(t),
            };
            match next {
                Some(n) if n != t => t = self.u.dealias(n),
                _ => break,
            }
        }
        t
    }

    /// The right-hand side of opaque type reference `t`, applied to its arguments.
    fn opaque_expansion(&mut self, t: TypeId) -> Option<TypeId> {
        let (tycon, args) = match self.u.types().get(t).clone() {
            Type::Applied(tycon, args) => (tycon, args),
            _ => (t, Vec::new()),
        };
        let s = self.referenced(tycon)?;
        if self.u.symbol(s).kind != Kind::Type || !self.u.symbol(s).flags.has(tasty::tags::OPAQUE) {
            return None;
        }
        self.complete(s);
        let rhs = self.u.opaque_alias(s).unwrap_or_else(|| self.u.info(s));
        let rhs = match *self.u.types().get(rhs) {
            Type::Alias(a) => a,
            Type::Bounds(..) => return None,
            _ => rhs,
        };
        if args.is_empty() {
            return Some(rhs);
        }
        let applied = self.u.intern_type(Type::Applied(rhs, args));
        Some(self.u.beta_reduced(applied))
    }
}
