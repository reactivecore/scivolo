// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//   compiler/src/dotty/tools/dotc/core/TypeUtils.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Named tuples `(a: A, b: B)` as `NamedTuple.NamedTuple[("a", "b"), (A, B)]`: types, literals
//! cast from their values tuple, and element selection `x.a` as `._1` on the values tuple.

use super::small_tuples::MAX_TUPLE_ARITY;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, TName, Tree};
use tasty::{Type, TypeId};

const NAMED_TUPLE: &str = "scala.NamedTuple$.NamedTuple";

impl Typer<'_> {
    /// The names and values of tuple `trees` if any element is a named argument; `Err` if only
    /// some are, a name repeats or there are more than 22.
    pub fn named_elements(&self, trees: List) -> Option<Result<Vec<(String, NodeId)>, &'static str>> {
        let elems = self.ast.items(trees);
        let named: Vec<(String, NodeId)> = elems
            .iter()
            .filter_map(|&e| match *self.ast.tree(e) {
                Tree::NamedArg { name, arg } => Some((self.text(name.name).to_owned(), arg)),
                _ => None,
            })
            .collect();
        match named.len() {
            0 => None,
            n if n < elems.len() => Some(Err("tuple of named and unnamed elements")),
            n if n > MAX_TUPLE_ARITY => Some(Err("named tuple of more than 22 elements")),
            _ if named.iter().enumerate().any(|(i, (n, _))| named[..i].iter().any(|(m, _)| m == n)) => {
                Some(Err("duplicate tuple element name"))
            }
            _ => Some(Ok(named)),
        }
    }

    /// Named tuple type `id` (dotty's `desugar.tuple` of a type).
    pub fn typed_named_tuple_type(&mut self, id: NodeId, elems: Vec<(String, NodeId)>, ctx: Ctx) -> TNodeId {
        let names: Vec<String> = elems.iter().map(|(n, _)| n.clone()).collect();
        let values: Vec<TypeId> = elems
            .iter()
            .map(|&(_, e)| {
                let typed = self.typed_type(e, ctx);
                self.tpd.tpe(typed)
            })
            .collect();
        match self.named_tuple_type(&names, &values) {
            Some(t) => self.type_tree(t, self.ast.span(id), false),
            None => self.unsupported(id, "named tuple classes missing from the library"),
        }
    }

    /// Named tuple literal `id`: `TupleN(…)` typed against `pt` stripped of its names, cast to the
    /// named tuple of its element types (dotty's `NamedTuple.build` after inlining).
    pub fn typed_named_tuple(
        &mut self,
        id: NodeId,
        elems: Vec<(String, NodeId)>,
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let names: Vec<String> = elems.iter().map(|(n, _)| n.clone()).collect();
        let args: Vec<NodeId> = elems.iter().map(|&(_, e)| e).collect();
        let span = self.ast.span(id);
        let module = TName { name: self.name(&format!("scala.Tuple{}", args.len())), is_type: false };
        let fun = self.ast.node(Tree::TypedSplice { sym: module });
        self.ast.with_span(fun, self.ast.span(args[0]).start_pos());
        let args = self.ast.list(&args);
        let app = self.ast.node(Tree::Apply { fun, args });
        self.ast.with_span(app, span);
        let named_pt = match pt {
            Pt::Type(t) => self.named_tuple_parts(t).filter(|(n, _)| *n == names).map(|(_, v)| (t, v)),
            _ => None,
        };
        let values_pt = named_pt.as_ref().and_then(|(_, v)| self.tuple_class_type(v));
        let typed = self.typed(app, values_pt.map_or(Pt::Any, Pt::Type), ctx);
        if let Some((t, _)) = named_pt {
            return self.type_cast(typed, t);
        }
        let tpe = self.tpd.tpe(typed);
        let widened = self.u.widen(tpe);
        let Some(values) = self.elements_of(widened) else {
            return self.unsupported(id, "named tuple of an untupled value");
        };
        match self.named_tuple_type(&names, &values) {
            Some(to) => self.type_cast(typed, to),
            None => self.unsupported(id, "named tuple classes missing from the library"),
        }
    }

    /// `qual.name` as `qual.$asInstanceOf$[V]._i` if `qual` is a named tuple with element `name`
    /// (dotty's `tryNamedTupleSelection`).
    pub fn named_tuple_select(
        &mut self,
        id: NodeId,
        qual: TNodeId,
        name: TName,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        if name.is_type {
            return None;
        }
        let tpe = self.tpd.tpe(qual);
        let widened = self.u.widen(tpe);
        let (names, values) = self.named_tuple_parts(widened)?;
        let i = names.iter().position(|n| n == self.text(name.name))?;
        let values = self.tuple_class_type(&values)?;
        let cast = self.type_cast(qual, values);
        let field = format!("_{}", i + 1);
        let sym =
            self.type_members_in(values, &field, ctx).into_iter().find(|&s| !self.u.is_type_symbol(s))?;
        let tpe = self.ref_type(Some(values), sym);
        let name = TName { name: self.names.intern(&field), is_type: false };
        Some(self.tpd.node(T::Select { qualifier: cast, name }, self.ast.span(id), Some(tpe), Some(sym)))
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

    /// The values tuple of `t` widened if that is a named tuple, else `t` (dotty's
    /// `stripNamedTuple`).
    pub fn strip_named_tuple(&mut self, t: TypeId) -> TypeId {
        let widened = self.u.widen(t);
        self.named_tuple_args(widened).map_or(t, |(_, values)| values)
    }

    /// The names and values type arguments of named tuple `t`.
    fn named_tuple_args(&mut self, t: TypeId) -> Option<(TypeId, TypeId)> {
        let nt = self.resolve_full_name(NAMED_TUPLE, true)?;
        let t = self.u.dealias(t);
        let Type::Applied(tycon, args) = self.u.types().get(t).clone() else { return None };
        let [names, values] = args[..] else { return None };
        (self.referenced(tycon) == Some(nt)).then_some((names, values))
    }

    /// `NamedTuple[(names…), (values…)]` with the names as string constant types.
    fn named_tuple_type(&mut self, names: &[String], values: &[TypeId]) -> Option<TypeId> {
        let nt = self.resolve_full_name(NAMED_TUPLE, true)?;
        let nt = crate::defn::type_ref(self.u, nt);
        let names: Vec<TypeId> = names.iter().map(|n| self.u.intern_type(Type::Str(n.clone()))).collect();
        let names = self.tuple_class_type(&names)?;
        let values = self.tuple_class_type(values)?;
        Some(self.u.intern_type(Type::Applied(nt, vec![names, values])))
    }

    /// The element types of tuple type `t`: a `TupleN` or a `*:` chain.
    fn elements_of(&mut self, t: TypeId) -> Option<Vec<TypeId>> {
        let d = self.u.dealias(t);
        if let Type::Applied(c, args) = self.u.types().get(d).clone()
            && self
                .u
                .class_symbol(c)
                .is_some_and(|c| self.u.full_name(c) == format!("scala.Tuple{}", args.len()))
        {
            return Some(args);
        }
        self.tuple_elements(t)
    }

    /// `TupleN[elems…]`.
    fn tuple_class_type(&mut self, elems: &[TypeId]) -> Option<TypeId> {
        if elems.is_empty() || elems.len() > MAX_TUPLE_ARITY {
            return None;
        }
        let class = self.resolve_full_name(&format!("scala.Tuple{}", elems.len()), true)?;
        let class = crate::defn::type_ref(self.u, class);
        Some(self.u.intern_type(Type::Applied(class, elems.to_vec())))
    }
}
