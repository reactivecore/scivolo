// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Types as `TypeRepr.toString` shows them (dotty's `Type.toString`).

use super::literals::constant_value;
use super::printed::Printed;
use super::type_string_refs::package_named;
use crate::host::Host;
use tasty::{Binder, Kind, Type, TypeId};

pub type Shown = Result<(), &'static str>;

pub const REFUSED: &str = "TypeRepr.toString of a type the frontend does not print";

impl Host {
    /// Type `t` as `TypeRepr.toString` shows it.
    pub fn type_string(&mut self, t: TypeId) -> Result<Printed, &'static str> {
        let mut text = Text { h: self, binders: Vec::new(), out: Printed::default() };
        text.tpe(t)?;
        Ok(text.out)
    }
}

/// A type printer, the binders it is inside (their parameter names, whether of terms) and the
/// text printed.
pub struct Text<'h> {
    pub h: &'h mut Host,
    binders: Vec<(Binder, Vec<String>, bool)>,
    pub out: Printed,
}

impl Text<'_> {
    fn tpe(&mut self, t: TypeId) -> Shown {
        match self.h.u.ty(t).clone() {
            Type::NoPrefix => self.out.push_str("NoPrefix"),
            Type::Package(p) => self.package(p, false)?,
            Type::PackageName(n) => self.out.push_str(&package_named(&n, false)),
            Type::Sym(prefix, s) => self.sym_ref(prefix, s)?,
            Type::Local(_, name) => self.out.push_str(&format!("TypeRef(NoPrefix,type {name})")),
            Type::TypeRef(p, n) => self.node("TypeRef", &[p], &format!(",{n}"))?,
            Type::TermRef(p, n) => match self.h.u.resolve_ref(t) {
                Some(s) if self.h.u.symbol(s).kind == Kind::Package => self.package(s, false)?,
                _ => self.node("TermRef", &[p], &format!(",{n}"))?,
            },
            Type::This(c) => self.this(c)?,
            Type::Super(a, b) => self.node("SuperType", &[a, b], "")?,
            Type::Applied(tycon, args) => {
                self.out.push_str("AppliedType(");
                self.tpe(tycon)?;
                self.out.push(',');
                self.list(&args)?;
                self.out.push(')');
            }
            Type::Alias(a) => self.node("TypeAlias", &[a], "")?,
            Type::Bounds(lo, hi) => self.node("TypeBounds", &[lo, hi], "")?,
            Type::And(a, b) => self.node("AndType", &[a, b], "")?,
            Type::Or(a, b, _) => self.node("OrType", &[a, b], "")?,
            Type::ByName(r) => self.node("ExprType", &[r], "")?,
            Type::Refined(parent, name, info) => {
                self.out.push_str("RefinedType(");
                self.tpe(parent)?;
                self.out.push_str(&format!(",{name},"));
                self.tpe(info)?;
                self.out.push(')');
            }
            Type::Method { binder, params, result, implicit, contextual } => {
                let what = match (contextual, implicit) {
                    (true, _) => "ContextualMethodType",
                    (_, true) => "ImplicitMethodType",
                    _ => "MethodType",
                };
                self.lambda(what, binder, &params, result, true)?
            }
            Type::Poly { binder, params, result } => {
                self.lambda("PolyType", binder, &params, result, false)?
            }
            Type::Lambda { binder, params, result } => {
                self.lambda("HKTypeLambda", binder, &params, result, false)?
            }
            Type::Param(b, i) => {
                let (_, names, terms) = self.binders.iter().rev().find(|(x, ..)| *x == b).ok_or(REFUSED)?;
                let name = names.get(i as usize).ok_or(REFUSED)?;
                let text = format!("{}ParamRef({name})", if *terms { "Term" } else { "Type" });
                self.out.push_str(&text);
            }
            Type::Match { bound, scrutinee, cases } => {
                self.out.push_str("MatchType(");
                self.separated(&[bound, scrutinee], ",")?;
                self.out.push(',');
                self.list(&cases)?;
                self.out.push(')');
            }
            Type::MatchCase(pat, body) => {
                let runtime = "TermRef(ThisType(TypeRef(NoPrefix,module class scala)),object runtime)";
                self.out.push_str(&format!("AppliedType(TypeRef({runtime},MatchCase),"));
                self.list(&[pat, body])?;
                self.out.push(')');
            }
            Type::Constant(tag, bits) => {
                self.out.push_str("ConstantType(Constant(");
                constant_value(tag, bits, &mut self.out).ok_or(REFUSED)?;
                self.out.push_str("))");
            }
            Type::Str(s) => self.out.push_str(&format!("ConstantType(Constant({s}))")),
            Type::Wildcard(None) => self.out.push_str("WildcardType(NoType)"),
            Type::Wildcard(Some(b)) => self.node("WildcardType", &[b], "")?,
            _ => return Err(REFUSED),
        }
        Ok(())
    }

    /// `what(parts…tail)`, the parts separated by commas.
    pub fn node(&mut self, what: &str, parts: &[TypeId], tail: &str) -> Shown {
        self.out.push_str(what);
        self.out.push('(');
        self.separated(parts, ",")?;
        self.out.push_str(tail);
        self.out.push(')');
        Ok(())
    }

    fn separated(&mut self, ts: &[TypeId], sep: &str) -> Shown {
        for (i, &t) in ts.iter().enumerate() {
            if i > 0 {
                self.out.push_str(sep);
            }
            self.tpe(t)?;
        }
        Ok(())
    }

    fn list(&mut self, ts: &[TypeId]) -> Shown {
        self.out.push_str("List(");
        self.separated(ts, ", ")?;
        self.out.push(')');
        Ok(())
    }

    fn lambda(
        &mut self,
        what: &str,
        binder: Binder,
        params: &[(String, TypeId)],
        result: TypeId,
        terms: bool,
    ) -> Shown {
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        self.out.push_str(&format!("{what}(List({}), ", names.join(", ")));
        self.binders.push((binder, names, terms));
        let infos: Vec<TypeId> = params.iter().map(|&(_, p)| p).collect();
        let shown = self.list(&infos).and_then(|_| {
            self.out.push_str(", ");
            self.tpe(result)
        });
        self.binders.pop();
        shown?;
        self.out.push(')');
        Ok(())
    }
}
