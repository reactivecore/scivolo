// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/Symbols.scala
//! Types as `TypeRepr.toString` shows them (dotty's `Type.toString`).

use super::literals::constant_value;
use crate::host::Host;
use tasty::{Binder, Kind, SymbolId, Type, TypeId};

type Shown = Result<String, &'static str>;

const REFUSED: &str = "TypeRepr.toString of a type the frontend does not print";

/// The reference to package `name` owned by package `owner`, as its `TermRef` (dotty's
/// `termRef`) or as its class's `this`.
fn package_ref(owner: &str, name: &str, this: bool) -> String {
    let owner_this = format!("ThisType(TypeRef(NoPrefix,module class {owner}))");
    match this {
        true => format!("ThisType(TypeRef({owner_this},module class {name}))"),
        false => format!("TermRef({owner_this},object {name})"),
    }
}

/// The reference to the package of qualified name `name`, as `package_ref`.
fn package_named(name: &str, this: bool) -> String {
    match name.rsplit_once('.') {
        Some((owner, last)) => package_ref(owner.rsplit('.').next().unwrap_or(owner), last, this),
        None => package_ref("<root>", name, this),
    }
}

fn list(items: &[String]) -> String {
    format!("List({})", items.join(", "))
}

impl Host {
    /// Type `t` as `TypeRepr.toString` shows it.
    pub fn type_string(&mut self, t: TypeId) -> Shown {
        Text { h: self, binders: Vec::new() }.tpe(t)
    }
}

/// A type printer and the binders it is inside: their parameter names, whether of terms.
struct Text<'h> {
    h: &'h mut Host,
    binders: Vec<(Binder, Vec<String>, bool)>,
}

impl Text<'_> {
    fn tpe(&mut self, t: TypeId) -> Shown {
        let node =
            |s: &mut Self, what: &str, parts: &[TypeId]| Ok(format!("{what}({})", s.types(parts)?.join(",")));
        Ok(match self.h.u.types().get(t).clone() {
            Type::NoPrefix => "NoPrefix".to_owned(),
            Type::Package(p) => self.package(p, false)?,
            Type::PackageName(n) => package_named(&n, false),
            Type::Sym(prefix, s) => self.sym_ref(prefix, s)?,
            Type::Local(_, name) => format!("TypeRef(NoPrefix,type {name})"),
            Type::TypeRef(p, n) => format!("TypeRef({},{n})", self.tpe(p)?),
            Type::TermRef(p, n) => match self.h.u.resolve_ref(t) {
                Some(s) if self.h.u.symbol(s).kind == Kind::Package => self.package(s, false)?,
                _ => format!("TermRef({},{n})", self.tpe(p)?),
            },
            Type::This(c) => self.this(c)?,
            Type::Super(a, b) => node(self, "SuperType", &[a, b])?,
            Type::Applied(tycon, args) => {
                let args = self.types(&args)?;
                format!("AppliedType({},{})", self.tpe(tycon)?, list(&args))
            }
            Type::Alias(a) => node(self, "TypeAlias", &[a])?,
            Type::Bounds(lo, hi) => node(self, "TypeBounds", &[lo, hi])?,
            Type::And(a, b) => node(self, "AndType", &[a, b])?,
            Type::Or(a, b, _) => node(self, "OrType", &[a, b])?,
            Type::ByName(r) => node(self, "ExprType", &[r])?,
            Type::Refined(parent, name, info) => {
                format!("RefinedType({},{name},{})", self.tpe(parent)?, self.tpe(info)?)
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
                format!("{}ParamRef({name})", if *terms { "Term" } else { "Type" })
            }
            Type::Match { bound, scrutinee, cases } => {
                let cases = self.types(&cases)?;
                format!("MatchType({},{},{})", self.tpe(bound)?, self.tpe(scrutinee)?, list(&cases))
            }
            Type::MatchCase(pat, body) => {
                let runtime = "TermRef(ThisType(TypeRef(NoPrefix,module class scala)),object runtime)";
                let parts = self.types(&[pat, body])?;
                format!("AppliedType(TypeRef({runtime},MatchCase),{})", list(&parts))
            }
            Type::Constant(tag, bits) => {
                format!("ConstantType(Constant({}))", constant_value(tag, bits).ok_or(REFUSED)?)
            }
            Type::Str(s) => format!("ConstantType(Constant({s}))"),
            Type::Wildcard(None) => "WildcardType(NoType)".to_owned(),
            Type::Wildcard(Some(b)) => node(self, "WildcardType", &[b])?,
            _ => return Err(REFUSED),
        })
    }

    fn types(&mut self, ts: &[TypeId]) -> Result<Vec<String>, &'static str> {
        ts.iter().map(|&t| self.tpe(t)).collect()
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
        self.binders.push((binder, names.clone(), terms));
        let infos: Result<Vec<String>, _> = params.iter().map(|&(_, p)| self.tpe(p)).collect();
        let result = self.tpe(result);
        self.binders.pop();
        Ok(format!("{what}({}, {}, {})", list(&names), list(&infos?), result?))
    }

    /// Package `p` as `package_ref` shows it; the root as the empty package's `this`.
    fn package(&mut self, p: SymbolId, this: bool) -> Shown {
        let root = self.h.u.root();
        if p == root {
            return Ok(package_ref("<root>", "<empty>", true));
        }
        let owner = self.h.u.symbol(p).owner.ok_or(REFUSED)?;
        let owner = if owner == root { "<root>" } else { &self.h.u.symbol(owner).name };
        Ok(package_ref(owner, &self.h.u.symbol(p).name, this))
    }

    /// The `this` of class or package `c`.
    fn this(&mut self, c: TypeId) -> Shown {
        match self.h.u.types().get(c).clone() {
            Type::Package(p) => self.package(p, true),
            Type::Sym(_, p) if self.h.u.symbol(p).kind == Kind::Package => self.package(p, true),
            Type::PackageName(n) => Ok(package_named(&n, true)),
            _ => Ok(format!("ThisType({})", self.tpe(c)?)),
        }
    }

    /// A reference to `s` (dotty's `designatorFor`): by name if its prefix is its owner's `this`,
    /// else by symbol; without a prefix through its owner's `this` (dotty's `namedType`).
    fn sym_ref(&mut self, prefix: Option<TypeId>, s: SymbolId) -> Shown {
        let sym = self.h.u.symbol(s);
        let (kind, name, owner) = (sym.kind, sym.name.clone(), sym.owner);
        if kind == Kind::Package {
            return self.package(s, false);
        }
        let what = if kind.is_term() { "TermRef" } else { "TypeRef" };
        let symbol = format!("{} {name}", self.kind_string(s));
        let Some(prefix) = prefix else {
            let prefix = match owner {
                Some(o) if self.h.u.symbol(o).kind == Kind::Package => {
                    let name = if o == self.h.u.root() { "<empty>" } else { &self.h.u.symbol(o).name };
                    format!("ThisType(TypeRef(NoPrefix,module class {name}))")
                }
                Some(o) if self.h.u.is_class(o) => format!("ThisType({})", self.sym_ref(None, o)?),
                _ => "NoPrefix".to_owned(),
            };
            return Ok(format!("{what}({prefix},{symbol})"));
        };
        let owner_this = match *self.h.u.types().get(prefix) {
            Type::This(c) => match *self.h.u.types().get(c) {
                Type::Package(p) => Some(p),
                _ => self.h.u.class_symbol(c),
            },
            _ => None,
        };
        let designator = if owner_this.is_some() && owner_this == owner { name } else { symbol };
        Ok(format!("{what}({},{designator})", self.tpe(prefix)?))
    }

    /// The kind of `s` as `SymDenotation.toString` names it.
    fn kind_string(&self, s: SymbolId) -> &'static str {
        let u = &self.h.u;
        match u.symbol(s).kind {
            _ if u.is_module_class(s) => "module class",
            _ if u.is_trait(s) => "trait",
            Kind::Class => "class",
            Kind::Type | Kind::TypeParam => "type",
            _ if u.is_module(s) => "object",
            Kind::Package => "object",
            Kind::Def => "method",
            _ => "val",
        }
    }
}
