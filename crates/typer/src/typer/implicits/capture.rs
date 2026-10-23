// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Wildcard capture of the type of a view's argument, an adapted tree or an overloaded
//! method's argument (dotty's `Inferencing.captureWildcards`).

use super::super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// `t` with each top-level wildcard argument `_ >: lo <: hi`, bounded also by its type
    /// parameter, replaced by the `CAP` member of a fresh skolem of type
    /// `scala.runtime.TypeBox[lo, hi]`; `None` if `t` has none.
    pub fn captured_wildcards(&mut self, t: TypeId) -> Option<TypeId> {
        self.captured_through(t, true)
    }

    /// `captured_wildcards` of an argument or adapted tree's type `t`; `None` for a stable path,
    /// which dotty's `compareCaptured` captures by selecting the type parameters on it, and for
    /// a Java class whose parameter has no upper bound (dotty's `FromJavaObject`).
    pub fn captured_args(&mut self, t: TypeId) -> Option<TypeId> {
        let t = self.zonk(t);
        if matches!(self.u.types().get(t), Type::Sym(..))
            && super::super::stability::is_stable_path(self.u, t)
        {
            return None;
        }
        self.captured_through(t, false)
    }

    fn captured_through(&mut self, t: TypeId, java_unbounded: bool) -> Option<TypeId> {
        let t = self.zonk(t);
        let t = self.u.widen(t);
        let t = self.u.dealias(t);
        let Type::Applied(tycon, args) = self.u.types().get(t).clone() else { return None };
        if !args.iter().any(|&a| matches!(self.u.types().get(a), Type::Bounds(..))) {
            return None;
        }
        let params = self.u.class_symbol(tycon).map(|c| self.u.type_params(c)).unwrap_or_default();
        if params.len() != args.len() {
            return None;
        }
        let type_box = self.resolve_full_name("scala.runtime.TypeBox", true)?;
        let cap = self.u.decl(type_box, "CAP").ok()?.first().copied()?;
        let java = !java_unbounded && self.u.class_symbol(tycon).is_some_and(|c| self.is_java_class_name(c));
        let mut captured = Vec::new();
        for (a, &p) in args.into_iter().zip(&params) {
            let a = self.with_param_bounds(a, Some(p), &params);
            let Type::Bounds(lo, hi) = *self.u.types().get(a) else {
                captured.push(a);
                continue;
            };
            if java && hi == tasty::ANY {
                return None;
            }
            let boxed = crate::defn::applied(self.u, type_box, vec![lo, hi]);
            let skolem = self.u.fresh_skolem(boxed);
            captured.push(self.u.intern_type(Type::Sym(Some(skolem), cap)));
        }
        Some(self.u.intern_type(Type::Applied(tycon, captured)))
    }

    /// `tree` cast to its type with wildcards captured if that conforms to `pt` (dotty's
    /// `captureWildcardsCompat` in `adapt`).
    pub fn captured_cast(&mut self, tree: TNodeId, pt: TypeId) -> Option<Result<TNodeId, &'static str>> {
        let captured = self.captured_args(self.tpd.tpe(tree))?;
        self.conforms(captured, pt).then(|| self.cast_to_captured(tree, captured))
    }

    /// Argument `arg` of a by-name parameter, an error if its wildcards were captured (dotty's
    /// `checkNoWildcardCaptureForCBN`).
    pub fn by_name_uncaptured(&mut self, arg: TNodeId) -> TNodeId {
        let T::TypeApply { fun, .. } = *self.tpd.tree(arg) else { return arg };
        let T::Select { qualifier, .. } = *self.tpd.tree(fun) else { return arg };
        let tpe = self.tpd.tpe(arg);
        let Type::Applied(_, args) = self.u.types().get(tpe).clone() else { return arg };
        let cap =
            self.resolve_full_name("scala.runtime.TypeBox", true).and_then(|b| self.u.decl(b, "CAP").ok());
        let is_cap = |t: &Self, a: TypeId| matches!(t.u.types().get(a), Type::Sym(_, s) if cap.as_ref().is_some_and(|c| c.contains(s)));
        if !args.iter().any(|&a| is_cap(self, a)) {
            return arg;
        }
        let shown = self.u.show_user(self.tpd.tpe(qualifier));
        let span = self.tpd.span(arg);
        let msg = format!(
            "argument for by-name parameter is not a value\nand contains wildcard arguments: {shown}\n\n\
             Assign it to a val and pass that instead.\n"
        );
        self.error_at(span, msg);
        self.error_tree(span)
    }

    /// `tree.$asInstanceOf$[to]`, at `tree`'s span made synthetic, `to` at its end.
    pub fn cast_to_captured(&mut self, tree: TNodeId, to: TypeId) -> Result<TNodeId, &'static str> {
        let span = self.tpd.span(tree).to_synthetic();
        let any = self.u.builtins().any;
        let found = self.u.decl(any, "$asInstanceOf$").ok().and_then(|d| d.first().copied());
        let fun = self.build().select(tree, found.ok_or("Any.$asInstanceOf$")?, span);
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, span.end_pos(), Some(to), None);
        Ok(self.build().type_apply(fun, &[tpt], to, span))
    }
}
