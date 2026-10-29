// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Which implicits convert a tree (dotty's `candidateKind` for a view prototype): a method of one
//! explicit parameter, or a value, or a method's result after its implicit clauses, of a class
//! deriving from `Conversion` or `<:<`; and the `apply` of such a value.

use super::super::widening::widens;
use super::super::{Pt, Typer};
use super::candidates::Candidate;
use super::conversions::Goal;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::tags::EXTENSION;
use tasty::{Binding, Kind, SymbolId, Type, TypeId};

#[derive(Clone, Copy, PartialEq)]
pub enum ViewKind {
    /// A method applied to the tree.
    Method,
    /// A value whose `apply` is applied to the tree.
    Value,
    None,
}

impl Typer<'_> {
    /// How candidate `c` converts a tree.
    pub fn view_kind(&mut self, c: Candidate) -> ViewKind {
        let value = self.candidate_value(c);
        if !self.is_implicit(c.sym) {
            return ViewKind::None;
        }
        match value {
            Err(1) => ViewKind::Method,
            Ok(value) if self.is_view_type(value) => ViewKind::Value,
            _ => ViewKind::None,
        }
    }

    /// Conversion value `fun` applied to its implicit arguments and its `apply` selected, its type
    /// first constrained to take `tree` to `goal` (dotty's `typedImplicit` adapting it to
    /// `Conversion[from, pt]`).
    pub fn conversion_apply(
        &mut self,
        fun: TNodeId,
        tree: TNodeId,
        goal: Goal,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let t = self.tpd.tpe(fun);
        let info = self.method_info(t).unwrap_or(t);
        let mut value = self.strip_implicit(info);
        if let Type::ByName(r) = *self.u.types().get(value) {
            value = r;
        }
        let to = match goal {
            Goal::Conform(pt) => pt,
            Goal::Member(_) => self.u.intern_type(Type::Wildcard(None)),
        };
        let function = self.function_class(1).ok_or("Function1")?;
        let expected = self.u.intern_type(Type::Applied(function, vec![self.tpd.tpe(tree), to]));
        if !self.conforms(value, expected) {
            return Err("conversion of another type");
        }
        let at = self.tpd.span(tree).start_pos();
        let fun = self.with_implicit_args(fun, (at, at), ctx, 1)?;
        let fun = self.inlined_transparent(fun, Pt::Any, ctx).unwrap_or(fun);
        self.with_apply(fun, Pt::Any, ctx)
    }

    /// Whether candidate `c` gives a value with an extension method `name`.
    pub fn offers_extension(&mut self, c: Candidate, name: &str) -> Result<bool, &'static str> {
        let Ok(value) = self.candidate_value(c) else { return Ok(false) };
        Ok(!self.extension_members(value, name)?.is_empty())
    }

    /// The extension methods `name` of a value of type `value`; `Err` if members other than
    /// methods are named so too.
    pub fn extension_members(&mut self, value: TypeId, name: &str) -> Result<Vec<SymbolId>, &'static str> {
        let value = self.u.widen(value);
        let members = match self.u.class_symbol(value) {
            Some(k) => self.class_members(k, name),
            None => self.type_members(value, name),
        };
        let extensions: Vec<SymbolId> =
            members.iter().copied().filter(|&e| self.u.symbol(e).flags.has(EXTENSION)).collect();
        if !extensions.is_empty() && members.iter().any(|&m| self.u.symbol(m).kind != Kind::Def) {
            return Err("extension method of an implicit value among other members");
        }
        Ok(extensions)
    }

    /// Whether dotty ranks view `c` above each of `widened`, views that take the constant
    /// argument only widened: a value above methods; a method if its parameter's value class
    /// widens to theirs, and their owners do not derive from its owner.
    pub fn beats_widened(&mut self, c: Candidate, widened: &[Candidate]) -> bool {
        if self.view_kind(c) == ViewKind::Value {
            return widened.iter().all(|&w| self.view_kind(w) == ViewKind::Method);
        }
        let Some(p) = self.view_param_class(c) else { return widened.is_empty() };
        let owner = self.u.symbol(c.sym).owner;
        widened.iter().all(|&w| {
            let other = self.u.symbol(w.sym).owner;
            let deeper = matches!((owner, other), (Some(o), Some(x)) if o != x && self.u.derives_from(x, o));
            !deeper && self.view_param_class(w).is_some_and(|q| widens(&p, &q))
        })
    }

    /// The view of `widened` that dotty ranks above the others.
    pub fn best_widened(&mut self, widened: &[Candidate]) -> Option<Candidate> {
        widened.iter().copied().find(|&w| {
            let others: Vec<Candidate> = widened.iter().copied().filter(|&o| o != w).collect();
            self.beats_widened(w, &others)
        })
    }

    /// Whether the value class of `from` widens numerically to that of `to`.
    pub fn widens_to(&mut self, from: TypeId, to: TypeId) -> bool {
        let (Some(from), Some(to)) = (self.value_class(from), self.value_class(to)) else { return false };
        widens(&from, &to)
    }

    /// The value class of the parameter of view method `c`.
    fn view_param_class(&mut self, c: Candidate) -> Option<String> {
        let tpe = self.ref_type(c.prefix, c.sym);
        let info = self.method_info(tpe)?;
        let Some(Binding { params, implicit: false, .. }) = self.u.method(info) else {
            return None;
        };
        let [(_, p)] = params[..] else { return None };
        self.value_class(p)
    }

    /// Whether a value of type `t` derives from `Conversion` or `<:<`.
    pub fn is_view_type(&mut self, t: TypeId) -> bool {
        let t = self.u.widen(t);
        ["scala.Conversion", "scala.<:<"]
            .iter()
            .any(|v| self.resolve_full_name(v, true).is_some_and(|b| self.u.base_type(t, b).is_some()))
    }

    /// The type of the value candidate `c` gives after its type and implicit parameters, or the
    /// number of parameters of its first explicit clause.
    pub fn candidate_value(&mut self, c: Candidate) -> Result<TypeId, usize> {
        self.complete(c.sym);
        let tpe = self.ref_type(c.prefix, c.sym);
        let mut info = self.method_info(tpe).unwrap_or(tpe);
        if let Type::Poly { result, .. } = *self.u.types().get(info) {
            info = result;
        }
        if let Type::Method { ref params, implicit: false, .. } = *self.u.types().get(info) {
            return Err(params.len());
        }
        let value = self.strip_implicit(info);
        match *self.u.types().get(value) {
            Type::ByName(r) => Ok(r),
            Type::Method { .. } | Type::Poly { .. } => Err(0),
            _ => Ok(value),
        }
    }
}
