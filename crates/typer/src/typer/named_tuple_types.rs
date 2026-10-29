// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//   compiler/src/dotty/tools/dotc/core/TypeUtils.scala
//! The type `NamedTuple.NamedTuple[Names, Values]` of named tuples: recognised, taken apart into
//! names and value types, and built.

use super::Typer;
use super::small_tuples::MAX_TUPLE_ARITY;
use tasty::{Type, TypeId};

const NAMED_TUPLE: &str = "scala.NamedTuple$.NamedTuple";
/// How many elements a named tuple type may have.
const MAX_ELEMENTS: usize = 256;

impl Typer<'_> {
    /// Whether `t` widened is a named tuple (dotty's `isNamedTupleType`).
    pub fn is_named_tuple_type(&mut self, t: TypeId) -> bool {
        self.stripped_named_tuple(t).is_some()
    }

    /// The values tuple of `t` widened if it is a named tuple (dotty's `stripNamedTuple`).
    pub fn stripped_named_tuple(&mut self, t: TypeId) -> Option<TypeId> {
        let widened = self.u.widen(t);
        self.named_tuple_args(widened).map(|(_, values)| values)
    }

    /// The values tuple of `t` widened if that is a named tuple, else `t`.
    pub fn strip_named_tuple(&mut self, t: TypeId) -> TypeId {
        self.stripped_named_tuple(t).unwrap_or(t)
    }

    /// The names and values type arguments of named tuple `t`.
    fn named_tuple_args(&mut self, t: TypeId) -> Option<(TypeId, TypeId)> {
        let nt = self.resolve_full_name(NAMED_TUPLE, true)?;
        let t = self.u.normalized(t);
        let t = self.u.dealias(t);
        let Some((tycon, &[names, values])) = self.u.applied(t) else { return None };
        (self.referenced(tycon) == Some(nt)).then_some((names, values))
    }

    /// The names and value types of named tuple `t` (dotty's `namedTupleElementTypes`).
    pub fn named_tuple_parts(&mut self, t: TypeId) -> Option<(Vec<String>, Vec<TypeId>)> {
        let (names, values) = self.named_tuple_args(t)?;
        let names = self.elements_of(names)?;
        let names: Option<Vec<String>> = names
            .iter()
            .map(|&n| match self.u.types().get(n) {
                Type::Str(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        let values = self.elements_of(values)?;
        names.filter(|n| n.len() == values.len()).map(|n| (n, values))
    }

    /// `named_tuple_parts` of `t`, of an abstract type its upper bound's, of an intersection
    /// those of the sides that have them, of the same names, the values intersected, of a union
    /// those both sides have alike (dotty's
    /// `namedTupleElementTypes(derived = true)`).
    pub fn derived_named_tuple_parts(&mut self, t: TypeId) -> Option<(Vec<String>, Vec<TypeId>)> {
        if let Some(parts) = self.named_tuple_parts(t) {
            return Some(parts);
        }
        let tpe = self.u.types().get(t).clone();
        match tpe {
            Type::Sym(_, s) if self.u.is_type_symbol(s) && self.u.class_symbol(t).is_none() => {
                self.complete(s);
                let hi = crate::sealed::upper_bound(self.u, s);
                (hi != t).then(|| self.derived_named_tuple_parts(hi)).flatten()
            }
            Type::And(a, b) => match (self.derived_named_tuple_parts(a), self.derived_named_tuple_parts(b)) {
                (Some((names, left)), Some((other, right))) => (names == other).then(|| {
                    let values = left.iter().zip(&right).map(|(&l, &r)| self.u.intern_type(Type::And(l, r)));
                    (names, values.collect())
                }),
                (left, right) => left.or(right),
            },
            Type::Or(a, b, _) => {
                let left = self.derived_named_tuple_parts(a)?;
                (self.derived_named_tuple_parts(b)? == left).then_some(left)
            }
            _ => None,
        }
    }

    /// `NamedTuple.NamedTuple`, selected from the object `NamedTuple`.
    pub fn named_tuple_ref(&mut self) -> Option<TypeId> {
        let nt = self.resolve_full_name(NAMED_TUPLE, true)?;
        let module = self.u.symbol(nt).owner.and_then(|o| self.u.source_module(o))?;
        let prefix = crate::defn::type_ref(self.u, module);
        Some(self.u.intern_type(Type::Sym(Some(prefix), nt)))
    }

    /// `NamedTuple[(names…), (values…)]` with the names as string constant types.
    pub fn named_tuple_type(&mut self, names: &[String], values: &[TypeId]) -> Option<TypeId> {
        let nt = self.named_tuple_ref()?;
        let names: Vec<TypeId> = names.iter().map(|n| self.u.intern_type(Type::Str(n.clone()))).collect();
        let names = self.values_type(&names)?;
        let values = self.values_type(values)?;
        Some(self.u.intern_type(Type::Applied(nt, vec![names, values])))
    }

    /// `t` with each `NamedTuple.From[C]` of a case class `C` reduced to the named tuple of its
    /// fields (dotty's `From` reduction).
    pub fn with_case_fields(&mut self, mut t: TypeId) -> TypeId {
        let Some(from) = self.resolve_full_name("scala.NamedTuple$.From", true) else { return t };
        let mut applied = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            if let Type::Applied(tycon, ref args) = *u.types().get(part)
                && let [arg] = args[..]
            {
                applied.push((part, tycon, arg));
            }
            None
        });
        for (part, tycon, arg) in applied {
            if self.referenced(tycon) != Some(from) {
                continue;
            }
            let Some(fields) = self.case_fields_tuple(arg) else { continue };
            t = self.u.map_type(t, &mut |_, p| (p == part).then_some(fields));
        }
        t
    }

    /// The named tuple of the fields of case class type `c`; `c` widened if a named tuple.
    fn case_fields_tuple(&mut self, c: TypeId) -> Option<TypeId> {
        use tasty::tags::{CASE, CASEaccessor};
        let widened = self.u.widen(c);
        if self.named_tuple_args(widened).is_some() {
            return Some(widened);
        }
        let class = self.u.class_symbol(c).filter(|&k| self.u.symbol(k).flags.has(CASE))?;
        let decls = self.u.symbol(class).decls.clone();
        let fields: Vec<_> = decls
            .into_iter()
            .filter(|&d| {
                self.u.symbol(d).kind == tasty::Kind::Val && self.u.symbol(d).flags.has(CASEaccessor)
            })
            .collect();
        let names: Vec<String> = fields.iter().map(|&f| self.u.symbol(f).name.clone()).collect();
        let values: Vec<TypeId> = fields.iter().map(|&f| self.u.member_info(c, f)).collect();
        let names: Vec<TypeId> = names.into_iter().map(|n| self.u.intern_type(Type::Str(n))).collect();
        let nt = self.named_tuple_ref()?;
        let (names, values) = (self.nested_pairs(&names).ok()?, self.nested_pairs(&values).ok()?);
        Some(self.u.intern_type(Type::Applied(nt, vec![names, values])))
    }

    /// The tuple type of `elems`: `TupleN[elems…]`, above 22 elements their `*:` chain.
    pub fn values_type(&mut self, elems: &[TypeId]) -> Option<TypeId> {
        if elems.is_empty() || elems.len() > MAX_TUPLE_ARITY {
            return self.nested_pairs(elems).ok();
        }
        let class = self.resolve_full_name(&format!("scala.Tuple{}", elems.len()), true)?;
        let class = crate::defn::type_ref(self.u, class);
        Some(self.u.intern_type(Type::Applied(class, elems.to_vec())))
    }

    /// The element types of tuple type `t`: a `TupleN` or a `*:` chain, normalized at each step.
    pub fn elements_of(&mut self, t: TypeId) -> Option<Vec<TypeId>> {
        let pair = self.resolve_full_name("scala.*:", true)?;
        let (mut elems, mut t) = (Vec::new(), t);
        for _ in 0..MAX_ELEMENTS {
            t = self.u.normalized(t);
            let d = self.u.dealias(t);
            let Type::Applied(c, args) = self.u.types().get(d).clone() else { break };
            let class = self.u.class_symbol(c);
            if class.is_some_and(|c| self.u.full_name(c) == format!("scala.Tuple{}", args.len())) {
                elems.extend(args);
                return Some(elems);
            }
            let [head, tail] = args[..] else { break };
            if class != Some(pair) {
                break;
            }
            elems.push(head);
            t = tail;
        }
        let rest = self.tuple_elements(t)?;
        elems.extend(rest);
        Some(elems)
    }
}
