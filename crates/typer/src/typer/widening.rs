//! Numeric widening of the value classes, and its conversions `Int.int2long` and the like that
//! adapting a non-constant inserts (dotty's views from the value classes' companions).

use super::Typer;
use crate::tpd::TNodeId;
use syntax::ast::flags as f;
use tasty::{Kind, Type, TypeId};

/// The value classes, and which of them each converts to by numeric widening.
const WIDENINGS: [(&str, &[&str]); 7] = [
    ("scala.Byte", &["scala.Short", "scala.Int", "scala.Long", "scala.Float", "scala.Double"]),
    ("scala.Short", &["scala.Int", "scala.Long", "scala.Float", "scala.Double"]),
    ("scala.Char", &["scala.Int", "scala.Long", "scala.Float", "scala.Double"]),
    ("scala.Int", &["scala.Long", "scala.Float", "scala.Double"]),
    ("scala.Long", &["scala.Float", "scala.Double"]),
    ("scala.Float", &["scala.Double"]),
    ("scala.Double", &[]),
];
const OTHER_VALUE_CLASSES: [&str; 2] = ["scala.Boolean", "scala.Unit"];

/// Whether the value class of full name `name` is numeric.
pub fn is_numeric(name: &str) -> bool {
    name != "scala.Boolean" && name != "scala.Unit" && WIDENINGS.iter().any(|(n, _)| *n == name)
}

/// Whether value class `from` widens to value class `to`, both by full name.
pub fn widens(from: &str, to: &str) -> bool {
    WIDENINGS.iter().any(|(n, targets)| *n == from && targets.contains(&to))
}

impl Typer<'_> {
    /// The full name of the value class `t` widens to, if it is one.
    pub fn value_class(&mut self, t: TypeId) -> Option<String> {
        let widened = self.u.widen(t);
        let name = self.u.class_symbol(widened).map(|c| self.u.full_name(c))?;
        let value = WIDENINGS.iter().any(|(n, _)| *n == name) || OTHER_VALUE_CLASSES.contains(&name.as_str());
        value.then_some(name)
    }

    /// `tree` widened to the numeric class of `pt` by its class's companion conversion, e.g.
    /// `Int.int2long(tree)`; `None` if no widening applies or a script implicit might compete.
    pub fn numeric_view(&mut self, tree: TNodeId, pt: TypeId) -> Option<TNodeId> {
        let tpe = self.tpd.get(tree).tpe?;
        let (from, to) = (self.value_class(tpe)?, self.value_class(pt)?);
        let implicits = self.syms.flags.values().any(|fl| fl.is(f::IMPLICIT) || fl.is(f::GIVEN));
        if !widens(&from, &to) || implicits {
            return None;
        }
        let short = |n: &str| n.trim_start_matches("scala.").to_lowercase();
        let name = format!("{}2{}", short(&from), short(&to));
        let module = self.resolve_full_name(&from, false)?;
        let module_class = self.u.module_class(module)?;
        let conv = self
            .u
            .decl(module_class, &name)
            .ok()?
            .into_iter()
            .find(|&d| self.u.symbol(d).kind == Kind::Def)?;
        let prefix = crate::defn::type_ref(self.u, module);
        let fun_tpe = self.u.intern_type(Type::Sym(Some(prefix), conv));
        let info = self.method_info(fun_tpe)?;
        let result = self.u.method(info)?.result;
        if !self.conforms(result, pt) {
            return None;
        }
        let span = self.tpd.span(tree);
        let fun = self.build().ident(conv, fun_tpe, span.start_pos());
        Some(self.build().apply(fun, &[tree], result, span.to_synthetic()))
    }
}
