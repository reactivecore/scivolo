// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenStat.scala
//! Conditions on values resolved at link time, as `linktime if` (nscplugin's
//! `getLinktimeCondition`, `LinktimeProperty`).

use super::Gen;
use crate::{Result, unsupported};
use nir::{Bin, Comp, Cond, Val};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

pub const RESOLVED_AT_LINKTIME: &str = "scala.scalanative.unsafe.resolvedAtLinktime";

impl Gen<'_, '_> {
    /// The symbol a reference or call `t` refers to.
    pub fn referenced(&self, mut t: TNodeId) -> Option<SymbolId> {
        while let Tree::Apply { fun, .. } | Tree::TypeApply { fun, .. } = *self.l.tpd.tree(t) {
            t = fun;
        }
        self.l.tpd.sym(t)
    }

    /// The name and whether calculated of the link-time property `t` refers to, if any
    /// (nscplugin's `LinktimeProperty`): the annotation's string, else the method's mangled name.
    pub fn property(&mut self, t: TNodeId) -> Option<(String, bool)> {
        let sym = self.referenced(t)?;
        let given = if self.l.u.symbol(sym).tree.is_some() {
            if !self.l.u.is_annotated(sym, RESOLVED_AT_LINKTIME) {
                return None;
            }
            self.l.u.annotation_string(sym, RESOLVED_AT_LINKTIME)
        } else {
            let annotations = self.l.u.annotations(sym).to_vec();
            let class = annotations.into_iter().find(|&c| self.l.u.full_name(c) == RESOLVED_AT_LINKTIME)?;
            self.l.u.annotation_strings(sym, class).first().cloned().flatten()
        };
        Some(match given {
            Some(name) => (name, false),
            None => {
                let name = self.method_name(sym);
                (self.pool.mangle(name), true)
            }
        })
    }

    /// `prop comp value` at the span of `prop`'s tree.
    fn simple(&mut self, prop: TNodeId, comp: Comp, value: Val) -> Option<Cond> {
        let (name, _) = self.property(prop)?;
        let pos = self.pos_or(self.l.tpd.span(prop), self.m.pos);
        let property = self.str(&name);
        let value = self.val(value);
        Some(Cond::Simple { property, comp, value, pos })
    }

    /// Condition `cond` as a link-time condition, if it is one; fails for one mixing link-time
    /// and run-time conditions.
    pub fn linktime_condition(&mut self, cond: TNodeId) -> Result<Option<Cond>> {
        let span = self.l.tpd.span(cond);
        let Tree::Apply { fun, args } = *self.l.tpd.tree(cond) else { return Ok(None) };
        let args = self.l.tpd.items(args).to_vec();
        if args.is_empty() && self.property(fun).is_some() {
            return Ok(self.simple(fun, Comp::Ieq, Val::True));
        }
        let (Tree::Select { qualifier, .. }, Some(op)) = (self.l.tpd.tree(fun).clone(), self.l.tpd.sym(fun))
        else {
            return Ok(None);
        };
        let op = self.l.u.symbol(op).name.clone();
        match (op.as_str(), &args[..]) {
            ("unary_!", []) => {
                let Tree::Apply { fun: inner, args: inner_args } = *self.l.tpd.tree(qualifier) else {
                    return Ok(None);
                };
                let inner_args = self.l.tpd.items(inner_args).to_vec();
                if inner_args.is_empty() && self.property(inner).is_some() {
                    return Ok(self.simple(inner, Comp::Ieq, Val::False));
                }
                let (Tree::Select { qualifier: prop, .. }, &[arg]) =
                    (self.l.tpd.tree(inner).clone(), &inner_args[..])
                else {
                    return Ok(None);
                };
                let is_eq = self.l.tpd.sym(inner).is_some_and(|s| self.l.u.symbol(s).name == "==");
                match self.literal_value(arg) {
                    Some(value) if is_eq && self.property(prop).is_some() => {
                        Ok(self.simple(prop, comparison("!=", &value, span)?, value))
                    }
                    _ => Ok(None),
                }
            }
            (op, &[arg]) => {
                if let Some(value) = self.literal_value(arg)
                    && self.property(qualifier).is_some()
                {
                    return Ok(self.simple(qualifier, comparison(op, &value, span)?, value));
                }
                match (self.linktime_condition(qualifier)?, self.linktime_condition(arg)?) {
                    (None, None) => Ok(None),
                    (Some(left), Some(right)) => {
                        let bin = match op {
                            "&&" => Bin::And,
                            "||" => Bin::Or,
                            _ => return unsupported(span, format!("`{op}` of link-time conditions")),
                        };
                        let pos = self.pos_or(span, self.m.pos);
                        Ok(Some(Cond::Complex { bin, left: Box::new(left), right: Box::new(right), pos }))
                    }
                    _ => unsupported(span, "mixing link-time and run-time conditions"),
                }
            }
            _ => Ok(None),
        }
    }

    /// The value of literal `t`.
    fn literal_value(&mut self, t: TNodeId) -> Option<Val> {
        let Tree::Literal { value } = self.l.tpd.tree(t).clone() else { return None };
        let v = self.gen_literal(t, &value).ok()?;
        Some(self.pool.val(v).clone())
    }
}

/// The comparison of a link-time property by `op` with `value`.
fn comparison(op: &str, value: &Val, span: Span) -> Result<Comp> {
    let float = matches!(value, Val::Float(_) | Val::Double(_));
    let (int, fl) = match op {
        "==" => (Comp::Ieq, Comp::Feq),
        "!=" => (Comp::Ine, Comp::Fne),
        ">" => (Comp::Sgt, Comp::Fgt),
        ">=" => (Comp::Sge, Comp::Fge),
        "<" => (Comp::Slt, Comp::Flt),
        "<=" => (Comp::Sle, Comp::Fle),
        _ => return unsupported(span, format!("the link-time condition `{op}`")),
    };
    Ok(if float { fl } else { int })
}
