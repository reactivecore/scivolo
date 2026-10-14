//! Which implicits convert a tree (dotty's `candidateKind` for a view prototype): a method of one
//! explicit parameter, or a value, or a method's result after its implicit clauses, of a class
//! deriving from `Conversion` or `<:<`.

use super::super::Typer;
use super::super::widening::widens;
use super::candidates::Candidate;
use tasty::tags::EXTENSION;
use tasty::{Type, TypeId};

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

    /// Whether candidate `c` gives a value with an extension method `name`.
    pub fn offers_extension(&mut self, c: Candidate, name: &str) -> bool {
        let Ok(value) = self.candidate_value(c) else { return false };
        let value = self.u.widen(value);
        self.u.class_symbol(value).is_some_and(|k| {
            self.class_members(k, name).iter().any(|&e| self.u.symbol(e).flags.has(EXTENSION))
        })
    }

    /// Whether dotty ranks view method `c` above each of `widened`, views that take the constant
    /// argument only widened: its parameter's value class widens to theirs, and their owners do
    /// not derive from its owner.
    pub fn beats_widened(&mut self, c: Candidate, widened: &[Candidate]) -> bool {
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
        let Type::Method { params, implicit: false, .. } = self.u.types().get(info).clone() else {
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
    fn candidate_value(&mut self, c: Candidate) -> Result<TypeId, usize> {
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
