//! Types as text, close to Scala syntax: `scala.Option[B]`, `(f: A => B): R`, `[B] => …`.

use super::{Type, TypeId};
use crate::Universe;

impl Universe {
    pub fn show(&self, t: TypeId) -> String {
        let params = |ps: &[(String, TypeId)]| {
            ps.iter().map(|(n, t)| format!("{n}: {}", self.show(*t))).collect::<Vec<_>>().join(", ")
        };
        match self.types().get(t) {
            Type::NoPrefix => String::new(),
            Type::TypeRef(p, n) | Type::TermRef(p, n) => match self.show(*p) {
                prefix if prefix.is_empty() => n.clone(),
                prefix => format!("{prefix}.{n}"),
            },
            Type::Sym(Some(p), s) => match self.show(*p) {
                prefix if prefix.is_empty() => self.symbol(*s).name.clone(),
                prefix => format!("{prefix}.{}", self.symbol(*s).name),
            },
            Type::Sym(None, s) => self.symbol(*s).name.clone(),
            Type::Local(_, n) => n.clone(),
            Type::Package(p) => self.full_name(*p),
            Type::PackageName(p) => p.clone(),
            Type::This(c) => format!("{}.this", self.show(*c)),
            Type::Super(t, p) => format!("{}.super[{}]", self.show(*t), self.show(*p)),
            Type::Applied(c, args) => {
                format!(
                    "{}[{}]",
                    self.show(*c),
                    args.iter().map(|a| self.show(*a)).collect::<Vec<_>>().join(", ")
                )
            }
            Type::Alias(a) => format!("= {}", self.show(*a)),
            Type::Bounds(lo, hi) if lo == hi => self.show(*lo),
            Type::Bounds(lo, hi) => format!(">: {} <: {}", self.show(*lo), self.show(*hi)),
            Type::And(a, b) => format!("{} & {}", self.show(*a), self.show(*b)),
            Type::Or(a, b, _) => format!("{} | {}", self.show(*a), self.show(*b)),
            Type::Annotated(u, _) => self.show(*u),
            Type::ByName(u) => format!("=> {}", self.show(*u)),
            Type::Refined(p, n, i) => format!("{} {{ {n}: {} }}", self.show(*p), self.show(*i)),
            Type::Rec(_, p) => self.show(*p),
            Type::RecThis(_) => "this".into(),
            Type::Method { params: ps, result, implicit, contextual, .. } => {
                let using = match (*contextual, *implicit) {
                    (true, _) => "using ",
                    (false, true) => "implicit ",
                    _ => "",
                };
                format!("({using}{}): {}", params(ps), self.show(*result))
            }
            Type::Poly { params: ps, result, .. } => format!("[{}]{}", params(ps), self.show(*result)),
            Type::Lambda { params: ps, result, .. } => format!("[{}] =>> {}", params(ps), self.show(*result)),
            Type::Param(_, i) => format!("<param {i}>"),
            Type::Match(bound) => format!("<match <: {}>", self.show(*bound)),
            Type::Constant(tag, v) => format!("{}({v})", crate::tags::name(*tag)),
            Type::Str(s) => format!("{s:?}"),
            Type::Other(tag) => format!("<{}>", crate::tags::name(*tag)),
            Type::Var(v) => format!("?{}", v.0),
            Type::Skolem(b, info) => format!("(?{} : {})", b.1, self.show(*info)),
            Type::Wildcard(None) => "?".into(),
            Type::Wildcard(Some(b)) => format!("? {}", self.show(*b)),
            Type::Error => "<error>".into(),
        }
    }
}
