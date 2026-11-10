// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! Implicit arguments the compiler makes instead of finding them (dotty's `Synthesizer`): a
//! `CanEqual` (`can_equal.rs`), a `ClassTag` (`class_tags.rs`), a `Mirror`
//! (`mirrors/`), a `TypeTest` (`type_tests.rs`), a `TupledFunction`
//! (`tupled_functions.rs`), and a `ValueOf[T]` of a singleton `T`, `new ValueOf[T](v)`.

mod can_equal;
mod class_tag_erasure;
mod class_tags;
mod manifests;
mod mirrors;
mod tupled_functions;
mod type_tests;

use crate::ctx::{Ctx, Mode};
use crate::defn::{CAN_EQUAL, CLASS_TAG, MANIFEST, OPT_MANIFEST, TUPLED_FUNCTION, TYPE_TEST, VALUE_OF};
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::Typer;
use crate::typer::constants::of_type;
use crate::typer::paths::MAX_PATH_DEPTH;
use manifests::ManifestKind;
use syntax::ast::constant::Constant as C;
use syntax::ast::{NodeId, Span, Tree};
use tasty::{SymbolId, Type, TypeId};

/// Makes one implicit argument the compiler synthesizes instead of finding it (dotty's
/// `Synthesizer`).
#[derive(Clone, Copy)]
struct Synthesizer {
    /// Where its trees are placed.
    at: Span,
    /// The context its trees are typed and its searches run in.
    ctx: Ctx,
    /// How deep the search that asks for the argument nests; its own searches nest one deeper.
    depth: u32,
}

impl Typer<'_> {
    /// Synthesizes the argument of type `pt` at `at` in `ctx` if `pt` is one the compiler makes:
    /// a `Mirror`, `CanEqual`, `ClassTag`, `ValueOf`, `Manifest`, `OptManifest`, `TypeTest` or
    /// `TupledFunction`; `None` for any other type, and for a search whose context turns
    /// synthesis off.
    pub fn synthesized(
        &mut self,
        pt: TypeId,
        at: Span,
        ctx: Ctx,
        depth: u32,
    ) -> Result<Option<TNodeId>, &'static str> {
        Synthesizer { at, ctx, depth }.run(self, pt)
    }
}

impl Synthesizer {
    fn run(self, t: &mut Typer<'_>, pt: TypeId) -> Result<Option<TNodeId>, &'static str> {
        if self.depth == 0 && self.ctx.mode.is(Mode::NO_SYNTHESIS) {
            return Ok(None);
        }
        if let Some(mirror) = self.mirror(t, pt)? {
            return Ok(Some(mirror));
        }
        let pt = t.u.dealias(pt);
        let Type::Applied(tycon, args) = t.u.types().get(pt).clone() else { return Ok(None) };
        let Some(class) = t.u.class_symbol(tycon) else { return Ok(None) };
        match (t.u.full_name(class).as_str(), &args[..]) {
            (CAN_EQUAL, &[l, r]) => self.can_equal(t, l, r),
            (CLASS_TAG, &[arg]) => self.class_tag(t, arg),
            (VALUE_OF, &[arg]) => self.value_of(t, class, arg),
            (MANIFEST, &[arg]) => self.manifest(t, arg, ManifestKind::Full),
            (OPT_MANIFEST, &[arg]) => self.manifest(t, arg, ManifestKind::Opt),
            (TYPE_TEST, &[from, to]) => self.type_test(t, from, to),
            (TUPLED_FUNCTION, &[fun, tupled]) => self.tupled_function(t, fun, tupled),
            _ => Ok(None),
        }
    }

    /// `new ValueOf[t](v)` for the value `v` of singleton type `t`, `t` made fully defined (dotty's
    /// `synthesizedValueOf`).
    fn value_of(
        self,
        t: &mut Typer<'_>,
        class: SymbolId,
        tpe: TypeId,
    ) -> Result<Option<TNodeId>, &'static str> {
        let at = self.at;
        let tpe = t.fully_defined_as(tpe, true)?;
        let normalized = t.u.normalized(tpe);
        let dealiased = t.u.dealias(normalized);
        let value = match t.u.types().get(dealiased).clone() {
            Type::Constant(tag, bits) => of_type(tag, bits).map(|c| t.build().literal(c, at)),
            Type::Str(s) => Some(t.build().literal(C::String(s.encode_utf16().collect()), at)),
            Type::Sym(_, s) if s == t.defn.unit => Some(t.build().literal(C::Unit, at)),
            Type::Sym(_, s) if t.u.symbol(s).kind.is_term() => t.path_tree(dealiased, at, MAX_PATH_DEPTH),
            Type::This(_) => t.path_tree(dealiased, at, MAX_PATH_DEPTH),
            Type::TermRef(..) | Type::Skolem(..) => None,
            _ => return Ok(None),
        };
        let value = value.ok_or("ValueOf of a path outside the subset")?;
        let tpe = t.tpd.tpe(value);
        let ctor = t.u.decl(class, "<init>").ok().and_then(|d| d.first().copied()).ok_or("ValueOf")?;
        let raw = crate::defn::type_ref(t.u, class);
        let tpt = t.tpd.node(T::TypeTree { inferred: false }, at, Some(raw), None);
        let new = t.tpd.node(T::New { tpt }, at, Some(raw), None);
        let select = t.build().select(new, ctor, at);
        let result = crate::defn::applied(t.u, class, vec![tpe]);
        let binder = t.u.fresh_binder();
        let params = vec![("value".to_owned(), tpe)];
        let method =
            t.u.intern_type(Type::Method { binder, params, result, implicit: false, contextual: false });
        let targ = t.type_tree(tpe, at, false);
        let fun = t.build().type_apply(select, &[targ], method, at);
        Ok(Some(t.build().apply(fun, &[value], result, at)))
    }

    /// Adds untyped `tree` at `at` to the typer's syntax tree.
    fn node(self, t: &mut Typer<'_>, tree: Tree) -> NodeId {
        let id = t.ast.node(tree);
        t.ast.with_span(id, self.at);
        id
    }
}
