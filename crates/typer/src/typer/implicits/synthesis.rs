// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! Implicit arguments the compiler makes instead of finding them (dotty's `Synthesizer`): a
//! `CanEqual` (`can_equal.rs`), a `ClassTag` (`class_tags.rs`), a `Mirror`
//! (`product_mirrors.rs`), a `TypeTest` (`type_test_synthesis.rs`), and a `ValueOf[T]` of a singleton `T`, `new ValueOf[T](v)`.

use super::super::Typer;
use super::super::constants::of_type;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use syntax::ast::constant::Constant as C;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The synthesized argument of type `pt` at `at` in `ctx`, if `pt` is one the compiler makes.
    pub fn synthesized(
        &mut self,
        pt: TypeId,
        at: Span,
        ctx: Ctx,
        depth: u32,
    ) -> Result<Option<TNodeId>, &'static str> {
        if let Some(mirror) = self.synthesized_mirror(pt, at, ctx)? {
            return Ok(Some(mirror));
        }
        let Type::Applied(tycon, args) = self.u.types().get(pt).clone() else { return Ok(None) };
        let Some(class) = self.u.class_symbol(tycon) else { return Ok(None) };
        match (self.u.full_name(class).as_str(), &args[..]) {
            ("scala.CanEqual", &[l, r]) => self.synthesized_can_equal(l, r, at, ctx, depth),
            ("scala.reflect.ClassTag", &[arg]) => self.synthesized_class_tag(arg, at, ctx, depth).map(Some),
            ("scala.ValueOf", &[arg]) => self.synthesized_value_of(class, arg, at),
            ("scala.reflect.TypeTest", &[from, to]) => self.synthesized_type_test(from, to, at, ctx),
            _ => Ok(None),
        }
    }

    /// `new ValueOf[t](v)` for the value `v` of singleton type `t`, `t` made fully defined (dotty's
    /// `synthesizedValueOf`).
    fn synthesized_value_of(
        &mut self,
        class: SymbolId,
        t: TypeId,
        at: Span,
    ) -> Result<Option<TNodeId>, &'static str> {
        let t = self.fully_defined_as(t, true)?;
        let dealiased = self.u.dealias(t);
        let value = match self.u.types().get(dealiased).clone() {
            Type::Constant(tag, bits) => of_type(tag, bits).map(|c| self.build().literal(c, at)),
            Type::Str(s) => Some(self.build().literal(C::String(s.encode_utf16().collect()), at)),
            Type::Sym(_, s) if s == self.defn.unit => Some(self.build().literal(C::Unit, at)),
            Type::Sym(_, s) if self.u.symbol(s).kind.is_term() => self.path_tree(dealiased, at, 8),
            Type::This(_) => self.path_tree(dealiased, at, 8),
            Type::TermRef(..) | Type::Skolem(..) => None,
            _ => return Ok(None),
        };
        let value = value.ok_or("ValueOf of a path outside the subset")?;
        let t = self.tpd.tpe(value);
        let ctor = self.u.decl(class, "<init>").ok().and_then(|d| d.first().copied()).ok_or("ValueOf")?;
        let raw = crate::defn::type_ref(self.u, class);
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, at, Some(raw), None);
        let new = self.tpd.node(T::New { tpt }, at, Some(raw), None);
        let select = self.build().select(new, ctor, at);
        let result = crate::defn::applied(self.u, class, vec![t]);
        let binder = self.u.fresh_binder();
        let params = vec![("value".to_owned(), t)];
        let method =
            self.u.intern_type(Type::Method { binder, params, result, implicit: false, contextual: false });
        let targ = self.type_tree(t, at, false);
        let fun = self.build().type_apply(select, &[targ], method, at);
        Ok(Some(self.build().apply(fun, &[value], result, at)))
    }
}
