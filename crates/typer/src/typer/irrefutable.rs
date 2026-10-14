//! Selectors of pattern definitions and generators, ascribed `@RuntimeChecked`, and their
//! refutable patterns (dotty's `typedMatch` rewrite, `Checking.checkIrrefutable`,
//! `Space.isIrrefutable`).

use super::Typer;
use super::migrations::Migration;
use crate::tpd::{TNodeId, Tree as T};
use syntax::config::SourceVersion;
use tasty::{Const, Type, TypeId, constant};

pub const UNCHECKED: &str = "scala.unchecked";
pub const RUNTIME_CHECKED: &str = "scala.annotation.internal.RuntimeChecked";

/// Why a pattern may not match its selector.
pub enum Refutable {
    NonConforming(TNodeId, TypeId),
    Extractor(TNodeId),
    Unknown,
}

impl Typer<'_> {
    /// Selector `sel` of a pattern definition (`pat_def`) or generator whose pattern is `pat`,
    /// ascribed `@RuntimeChecked` unless annotated so or `@unchecked`; a refutable `pat` warned
    /// about or refused by the source version.
    pub fn runtime_checked(
        &mut self,
        sel: TNodeId,
        pat: TNodeId,
        pat_def: bool,
    ) -> Result<TNodeId, &'static str> {
        let sel_tpe = self.tpd.tpe(sel);
        if self.has_annotation(sel_tpe, UNCHECKED) || self.has_annotation(sel_tpe, RUNTIME_CHECKED) {
            return Ok(sel);
        }
        if matches!(self.u.types().get(sel_tpe), Type::Annotated(..)) {
            return Err("annotated selector of a pattern definition");
        }
        let error_from = if pat_def { SourceVersion::FUTURE_VERSION } else { SourceVersion::V3_4 };
        let migration = self.migration(SourceVersion::V3_2, error_from);
        if migration != Migration::Silent
            && let Err(reason) = self.check_irrefutable(pat, sel_tpe)
        {
            if migration == Migration::Error || matches!(reason, Refutable::Unknown) {
                return Err(if pat_def {
                    "refutable pattern definition"
                } else {
                    "refutable pattern in a generator"
                });
            }
            self.warn_refutable(reason, sel, pat_def);
        }
        let annot =
            self.resolve_full_name(RUNTIME_CHECKED, true).ok_or("RuntimeChecked missing from the library")?;
        let annot = crate::defn::type_ref(self.u, annot);
        let widened = self.u.widen(sel_tpe);
        let tpe = self.u.intern_type(Type::Annotated(widened, annot));
        let span = self.tpd.span(sel);
        let tpt = self.type_tree(tpe, span.end_pos(), true);
        Ok(self.tpd.node(T::Typed { expr: sel, tpt }, span.to_synthetic(), Some(tpe), None))
    }

    /// Whether `t` or a type it annotates carries annotation class `name`.
    fn has_annotation(&mut self, t: TypeId, name: &str) -> bool {
        let Type::Annotated(inner, annot) = *self.u.types().get(t) else { return false };
        self.u.class_symbol(annot).is_some_and(|c| self.u.full_name(c) == name)
            || self.has_annotation(inner, name)
    }

    /// The first reason dotty's `checkIrrefutable` finds why pattern `pat` may not match a value
    /// of type `pt`.
    fn check_irrefutable(&mut self, pat: TNodeId, pt: TypeId) -> Result<(), Refutable> {
        if self.has_annotation(pt, UNCHECKED) || self.has_annotation(pt, RUNTIME_CHECKED) {
            return Ok(());
        }
        match *self.tpd.tree(pat) {
            T::Bind { body, .. } | T::NamedArg { arg: body, .. } => self.check_irrefutable(body, pt),
            T::UnApply { fun, patterns, .. } => {
                self.check_conforms(pat, pt)?;
                let pats = self.tpd.items(patterns).to_vec();
                if !self.is_irrefutable_extractor(fun, pats.len()) {
                    return Err(Refutable::Extractor(pat));
                }
                let arg_types = self.extractor_arg_types(fun, pats.len()).ok_or(Refutable::Unknown)?;
                pats.into_iter().zip(arg_types).try_for_each(|(p, t)| self.check_irrefutable(p, t))
            }
            T::Alternative { trees } => {
                let alts = self.tpd.items(trees).to_vec();
                alts.into_iter().try_for_each(|a| self.check_irrefutable(a, pt))
            }
            T::Typed { expr, .. } => {
                self.check_conforms(pat, pt)?;
                self.check_irrefutable(expr, pt)
            }
            T::Ident { name } if self.text(name.name) == "_" => Ok(()),
            _ => self.check_conforms(pat, pt),
        }
    }

    /// Whether `pt`, or a type it widens to, conforms to the type of pattern `pat`.
    fn check_conforms(&mut self, pat: TNodeId, pt: TypeId) -> Result<(), Refutable> {
        let pat_tpe = self.tpd.tpe(pat);
        let mut t = pt;
        loop {
            if self.u.is_subtype(t, pat_tpe) {
                return Ok(());
            }
            let wide = self.u.widen(t);
            if wide == t {
                return Err(Refutable::NonConforming(pat, pt));
            }
            t = wide;
        }
    }

    /// Whether extractor `fun` with `n` patterns matches every value its parameter accepts
    /// (dotty's `Space.isIrrefutable`).
    fn is_irrefutable_extractor(&mut self, fun: TNodeId, n: usize) -> bool {
        let result = self.extractor_result(fun);
        let sym = self.tpd.sym(self.tpd.meth_part(fun));
        let case_companion = sym.is_some_and(|s| {
            let synthetic = self.u.symbol(s).flags.has(tasty::tags::SYNTHETIC);
            let owner = self.u.symbol(s).owner;
            let class = owner.and_then(|o| self.u.companion_class(o));
            synthetic && class.is_some_and(|c| self.u.symbol(c).flags.has(tasty::tags::CASE))
        });
        let some = self.resolve_full_name("scala.Some", true);
        self.u.class_symbol(result).is_some_and(|c| Some(c) == some)
            || self.is_bool_constant(result, true)
            || case_companion
            || self.seq_elem_type(result).is_some()
            || n > 0
                && self.derives_from_named(result, "scala.Product")
                && self.product_selectors(result).is_ok_and(|s| s.len() == n)
            || self
                .extractor_member(result, "isEmpty")
                .ok()
                .flatten()
                .is_some_and(|e| self.is_bool_constant(e, false))
            || self.derives_from_named(result, "scala.NonEmptyTuple")
    }

    /// The result type of extractor `fun`.
    fn extractor_result(&mut self, fun: TNodeId) -> TypeId {
        let fun_tpe = self.tpd.tpe(fun);
        let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
        let result = self.final_result(info);
        self.zonk(result)
    }

    fn is_bool_constant(&mut self, t: TypeId, value: bool) -> bool {
        let t = self.u.dealias(t);
        matches!(*self.u.types().get(t), Type::Constant(tag, bits) if constant(tag, bits) == Some(Const::Bool(value)))
    }

    /// The types extractor `fun` gives its `n` patterns (dotty's `UnapplyArgs.argTypes`).
    fn extractor_arg_types(&mut self, fun: TNodeId, n: usize) -> Option<Vec<TypeId>> {
        let result = self.extractor_result(fun);
        let seq =
            self.tpd.sym(self.tpd.meth_part(fun)).is_some_and(|s| self.u.symbol(s).name == "unapplySeq");
        if !seq {
            return self.unapply_arg_types(result, n).ok();
        }
        self.unapply_seq_arg_types(result, n).ok().map(|(types, _)| types)
    }
}
