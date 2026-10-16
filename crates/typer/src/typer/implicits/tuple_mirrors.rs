//! `Mirror`s of tuple types (dotty's `makeTupleProductMirror`): `new scala.runtime.TupleMirror(n)`
//! cast to the mirror type.

use super::super::Typer;
use super::super::small_tuples::MAX_TUPLE_ARITY;
use super::mirror_types::{MISSING, MirrorCore, MirrorFormal};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// The mirror of a tuple of `elems` for `formal`.
    pub fn tuple_mirror(
        &mut self,
        formal: &MirrorFormal,
        elems: &[TypeId],
        at: Span,
    ) -> Result<Option<TNodeId>, &'static str> {
        if matches!(self.u.types().get(formal.mirrored), Type::Lambda { .. }) {
            return Err("Mirror of a tuple type lambda");
        }
        let runtime = self.resolve_full_name("scala.runtime.TupleMirror", true).ok_or(MISSING)?;
        let labels: Vec<String> = (1..=elems.len()).map(|i| format!("_{i}")).collect();
        let elem_types = self.nested_pairs(elems)?;
        let elem_labels = self.label_pairs(&labels)?;
        let label = match elems.len() {
            n if n <= MAX_TUPLE_ARITY => format!("Tuple{n}"),
            _ => "Tuple".to_owned(),
        };
        let core = MirrorCore {
            parent: "scala.deriving.Mirror$.Product",
            mono: formal.mirrored,
            label: &label,
            elems: Some((elem_types, elem_labels)),
        };
        let cast = self.mirror_type(formal, core)?;
        let ctor = self.u.decl(runtime, "<init>").ok().and_then(|d| d.first().copied()).ok_or(MISSING)?;
        let raw = crate::defn::type_ref(self.u, runtime);
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, at, Some(raw), None);
        let new = self.tpd.node(T::New { tpt }, at, Some(raw), None);
        let select = self.build().select(new, ctor, at);
        let arity = self.build().literal(Constant::Int(elems.len() as i32), at);
        let created = self.build().apply(select, &[arity], raw, at);
        self.cast_to(created, cast, at).map(Some)
    }

    /// The element types of `t` if it is `TupleN[…]`.
    pub fn tuple_class_args(&mut self, t: TypeId) -> Option<Vec<TypeId>> {
        let d = self.u.dealias(t);
        let Type::Applied(f, args) = self.u.types().get(d).clone() else { return None };
        let c = self.u.class_symbol(f)?;
        (self.u.full_name(c) == format!("scala.Tuple{}", args.len())).then_some(args)
    }
}
