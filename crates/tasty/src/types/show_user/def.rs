//! Definitions (`dclText`), method and lambda types, bounds and refinements.

use super::{DOT, GLOBAL, Printer, simple};
use crate::tags::{CASE, LAZY, MUTABLE, OBJECT, TRAIT};
use crate::types::{Type, TypeId};
use crate::{Kind, SymbolId};

impl Printer<'_> {
    pub fn def(&mut self, s: SymbolId) -> String {
        let sym = self.u.symbol(s);
        let (kind, flags) = (sym.kind, sym.flags);
        let module = flags.has(OBJECT) && matches!(kind, Kind::Class | Kind::Val);
        let name = match kind == Kind::Class && module {
            true => sym.name.strip_suffix('$').unwrap_or(&sym.name).to_owned(),
            false => sym.name.clone(),
        };
        let owner_is_term = sym.owner.is_some_and(|o| self.u.symbol(o).kind.is_term());
        let key = match kind {
            _ if module && flags.has(CASE) => "case object",
            _ if module => "object",
            Kind::Class if flags.has(TRAIT) => "trait",
            Kind::Class if flags.has(CASE) => "case class",
            Kind::Class => "class",
            Kind::Type | Kind::TypeParam if owner_is_term => "",
            Kind::Type | Kind::TypeParam => "type",
            Kind::Package => "package",
            _ if flags.has(MUTABLE) => "var",
            Kind::Def => "def",
            Kind::Val => "val",
            Kind::Param => "",
        };
        let head: Vec<&str> = [if flags.has(LAZY) { "lazy" } else { "" }, key, &name]
            .into_iter()
            .filter(|w| !w.is_empty())
            .collect();
        let rest = match kind {
            _ if module => String::new(),
            Kind::Class | Kind::Package | Kind::TypeParam => String::new(),
            Kind::Type => match self.u.alias_of(s) {
                Some(alias) => {
                    let variances = self.u.variances(s);
                    self.alias(alias, &variances)
                }
                None => String::new(),
            },
            Kind::Def | Kind::Val | Kind::Param => {
                let info = self.u.info(s);
                match self.u.types().get(info).clone() {
                    Type::Method { .. } | Type::Poly { .. } if kind == Kind::Def => self.lambda(info),
                    Type::ByName(r) => format!(": {}", self.text(r, GLOBAL)),
                    _ => format!(": {}", self.text(info, GLOBAL)),
                }
            }
        };
        format!("{}{rest}", head.join(" "))
    }

    /// A method, polymorphic method or type lambda type, its parameters' names in scope.
    pub fn lambda(&mut self, t: TypeId) -> String {
        let (binder, params, result) = match self.u.types().get(t).clone() {
            Type::Method { binder, params, result, .. }
            | Type::Poly { binder, params, result }
            | Type::Lambda { binder, params, result } => (binder, params, result),
            _ => return self.text(t, GLOBAL),
        };
        self.binders.push((binder, params.iter().map(|(n, _)| n.clone()).collect()));
        let nested = matches!(self.u.types().get(result), Type::Method { .. } | Type::Poly { .. });
        let colon = if nested { "" } else { ": " };
        let text = match self.u.types().get(t).clone() {
            Type::Method { implicit, contextual, .. } => {
                let key = match (contextual, implicit) {
                    (true, _) => "using ",
                    (false, true) => "implicit ",
                    _ => "",
                };
                let params = self.params(&params);
                format!("({key}{params}){colon}{}", self.text(result, GLOBAL))
            }
            Type::Poly { .. } => {
                format!("[{}]{colon}{}", self.type_params(&params), self.text(result, GLOBAL))
            }
            _ => format!("[{}] =>> {}", self.type_params(&params), self.text(result, GLOBAL)),
        };
        self.binders.pop();
        text
    }

    pub fn params(&mut self, params: &[(String, TypeId)]) -> String {
        let texts: Vec<String> = params
            .iter()
            .map(|(n, t)| match self.u.types().get(*t).clone() {
                Type::ByName(r) => format!("{n}: => {}", self.text(r, GLOBAL)),
                _ => format!("{n}: {}", self.text(*t, GLOBAL)),
            })
            .collect();
        texts.join(", ")
    }

    pub fn type_params(&mut self, params: &[(String, TypeId)]) -> String {
        let texts: Vec<String> =
            params.iter().map(|(n, b)| format!("{}{}", simple(n), self.rhs(*b))).collect();
        texts.join(", ")
    }

    /// The right-hand side of a type definition: ` = T`, ` >: L <: H` without trivial bounds,
    /// the parameters of a type lambda before it.
    pub fn rhs(&mut self, t: TypeId) -> String {
        let depth = self.binders.len();
        let text = match self.u.types().get(t).clone() {
            Type::Alias(a) => self.alias(a, &[]),
            Type::Bounds(lo, hi) if lo == hi => self.alias(lo, &[]),
            Type::Bounds(lo, hi) => {
                let (lo, _) = self.lambda_params(lo, &[]);
                let (hi, params) = self.lambda_params(hi, &[]);
                let lo = match self.full_name_of(lo).as_deref() {
                    Some("scala.Nothing") => String::new(),
                    _ => format!(" >: {}", self.text(lo, GLOBAL)),
                };
                let hi = match self.full_name_of(hi).as_deref() {
                    Some("scala.Any") => String::new(),
                    _ => format!(" <: {}", self.text(hi, GLOBAL)),
                };
                format!("{params}{lo}{hi}")
            }
            Type::Lambda { result, .. } if matches!(self.u.types().get(result), Type::Bounds(..)) => {
                let (bounds, params) = self.lambda_params(t, &[]);
                format!("{params}{}", self.rhs(bounds))
            }
            Type::Lambda { .. } => self.alias(t, &[]),
            _ => format!(": {}", self.text(t, GLOBAL)),
        };
        self.binders.truncate(depth);
        text
    }

    /// ` = a`, the parameters of type lambda `a` before it with `variances`.
    fn alias(&mut self, a: TypeId, variances: &[i8]) -> String {
        let depth = self.binders.len();
        let (a, params) = self.lambda_params(a, variances);
        let text = format!("{params} = {}", self.text(a, GLOBAL));
        self.binders.truncate(depth);
        text
    }

    /// The result of type lambda `t` and its parameters `[X,+Y]`, `[_$1]` with `variances`, which stay
    /// in scope; `t` and nothing else.
    fn lambda_params(&mut self, t: TypeId, variances: &[i8]) -> (TypeId, String) {
        let Type::Lambda { binder, params, result } = self.u.types().get(t).clone() else {
            return (t, String::new());
        };
        let texts: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(i, (n, b))| {
                let sign = match variances.get(i) {
                    Some(1) => "+",
                    Some(-1) => "-",
                    _ => "",
                };
                format!("{sign}{n}{}", self.rhs(*b))
            })
            .collect();
        self.binders.push((binder, params.into_iter().map(|(n, _)| n).collect()));
        (result, format!("[{}]", texts.join(",")))
    }

    /// A refinement type `P{type T = A; def f: B}`.
    pub fn refined(&mut self, mut t: TypeId, prec: u8) -> String {
        let mut members = Vec::new();
        while let Type::Refined(parent, name, info) = self.u.types().get(t).clone() {
            members.push((name, info));
            t = parent;
        }
        if let [(name, info)] = &members[..]
            && name == "apply"
            && self.is_function_class(t)
        {
            return super::paren(GLOBAL, prec, self.as_function(*info));
        }
        let texts: Vec<String> = members
            .into_iter()
            .rev()
            .map(|(name, info)| match self.u.types().get(info).clone() {
                Type::Method { .. } | Type::Poly { .. } => format!("def {name}{}", self.lambda(info)),
                Type::ByName(r) => format!("def {name}: {}", self.text(r, GLOBAL)),
                Type::Alias(_) | Type::Bounds(..) => format!("type {name}{}", self.rhs(info)),
                _ => format!("val {name}: {}", self.text(info, GLOBAL)),
            })
            .collect();
        format!("{}{{{}}}", self.text(t, DOT), texts.join("; "))
    }
}
