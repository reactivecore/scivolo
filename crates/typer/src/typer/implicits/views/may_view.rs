// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Whether an implicit definition may convert a class to a type with a given member or to a given
//! type, or offer an extension method of that name (dotty's `candidateKind` for a view prototype),
//! decided without searching.

use super::super::super::Typer;
use super::super::super::did_you_mean::{KNOWN_JAVA, OBJECT_MEMBERS, is_jdk};
use super::super::super::java_members::java_members;
use super::super::super::type_ops::TypeOps;
use super::super::super::widening::{is_numeric, widens};
use super::Goal;
use super::view_kinds::is_view_type;
use tasty::{Binding, Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Tells whether member `m` of an object is an extension method named as `goal`'s member, or
    /// an implicit that may convert class `q` to fit `goal` or offer such an extension method, as
    /// `implicit def toRich(i: Int): RichInt` for `1.max` on `Int`.
    pub fn may_view(&mut self, m: SymbolId, q: SymbolId, goal: Goal) -> bool {
        let sym = self.u.symbol(m);
        if sym.is_extension() {
            return goal.member().is_some_and(|name| sym.name == name);
        }
        if !sym.is_implicit() || !matches!(sym.kind, Kind::Val | Kind::Def) {
            return false;
        }
        self.complete(m);
        if !self.u.symbol(m).is_implicit() {
            return false;
        }
        let info = self.u.info(m);
        let info = self.u.strip_poly(info);
        if let Some(Binding { params, result, implicit: false, .. }) = self.u.method(info) {
            let [(_, param)] = *params else { return false };
            let mut ty = self.types();
            let result = ty.strip_implicit(result);
            return ty.may_accept(param, q, matches!(goal, Goal::Conform(_)))
                && match goal {
                    Goal::Member(name) | Goal::Call(name, _) | Goal::Typed(name, _) => {
                        ty.may_have(result, param, name)
                    }
                    Goal::Conform(to) => ty.may_conform(result, param, to),
                };
        }
        let value = self.strip_implicit(info);
        let mut value = self.u.widen(value);
        while let Type::Refined(t, ..) | Type::Rec(_, t) | Type::Annotated(t, _) = *self.u.types().get(value)
        {
            value = t;
        }
        let Some(c) = self.u.class_symbol(value) else { return true };
        if let Some(name) = goal.member()
            && self.class_members(c, name).iter().any(|&e| self.u.symbol(e).is_extension())
        {
            return true;
        }
        let is_view = is_view_type(self, value);
        let identity = matches!(self.u.types().get(value), Type::Applied(_, args) if args.len() == 2 && args[0] == args[1]);
        is_view && !(identity && ["scala.<:<", "scala.=:="].contains(&&*self.u.full_name(c)))
    }
}

impl TypeOps<'_> {
    /// Whether class `q`, whose base classes the universe knows, may conform or widen
    /// numerically (only to a wider class if `widening`) to view parameter type `param`; not to a
    /// class of a package the universe lacks.
    fn may_accept(&mut self, param: TypeId, q: SymbolId, widening: bool) -> bool {
        let param = self.u.widen(param);
        let param = self.u.dealias(param);
        match *self.u.types().get(param) {
            Type::Or(a, b, _) => return self.may_accept(a, q, widening) || self.may_accept(b, q, widening),
            Type::Annotated(t, _) => return self.may_accept(t, q, widening),
            _ => {}
        }
        let tycon = match self.u.types().get(param) {
            Type::Applied(tycon, _) => *tycon,
            _ => param,
        };
        if let Type::TypeRef(p, n) = self.u.types().get(tycon).clone()
            && matches!(self.u.types().get(p), Type::Package(_) | Type::PackageName(_))
            && self.u.resolve(p, &n, true).is_none()
        {
            return false;
        }
        let Some(p) = self.direct_class(param) else { return true };
        let (from, to) = (self.u.full_name(q), self.u.full_name(p));
        let numeric = if widening { widens(&from, &to) } else { is_numeric(&from) && is_numeric(&to) };
        self.u.derives_from(q, p) || numeric
    }

    /// Whether result type `result` of a view of parameter type `param` may have member `name`.
    fn may_have(&mut self, result: TypeId, param: TypeId, name: &str) -> bool {
        let result = self.u.dealias(result);
        if result == param && matches!(self.u.types().get(result), Type::Param(..)) {
            return false;
        }
        let Some(c) = self.direct_class(result) else { return true };
        self.view_result_names(c).is_none_or(|names| names.iter().any(|n| n == name))
    }

    /// Whether result type `result` of a view of parameter type `param` may conform to `to`.
    fn may_conform(&mut self, result: TypeId, param: TypeId, to: TypeId) -> bool {
        let result = self.u.dealias(result);
        if result == param && matches!(self.u.types().get(result), Type::Param(..)) {
            return false;
        }
        let to = self.u.dealias(to);
        let (Some(r), Some(t)) = (self.direct_class(result), self.direct_class(to)) else { return true };
        let b = self.u.builtins();
        r == b.nothing || r == b.null || self.u.derives_from(r, t)
    }

    /// The names of the members of class `c`, and more; `None` where they are not all known.
    fn view_result_names(&mut self, c: SymbolId) -> Option<Vec<String>> {
        let mut names: Vec<String> = OBJECT_MEMBERS.split_whitespace().map(str::to_owned).collect();
        if let Some(members) = java_members(&self.u.full_name(c)) {
            names.extend(members.map(str::to_owned));
            return Some(names);
        }
        for base in self.u.linearization(c) {
            let full = self.u.full_name(base);
            if let Some(members) = java_members(&full) {
                names.extend(members.map(str::to_owned));
                continue;
            }
            if is_jdk(&full) && !KNOWN_JAVA.contains(&&*full) {
                return None;
            }
            let decls = self.u.decls(base).ok()?;
            names.extend(decls.iter().map(|&d| self.u.symbol(d).name.clone()));
            for p in self.u.parents(base) {
                if self.u.class_symbol(p).is_none() {
                    let parent = self.class_ref_name(p)?;
                    names.extend(java_members(&parent)?.map(str::to_owned));
                }
            }
        }
        Some(names)
    }

    /// The qualified name of the class type `t` refers to, applied or not, by a package prefix.
    fn class_ref_name(&mut self, t: TypeId) -> Option<String> {
        let tycon = match self.u.types().get(t) {
            Type::Applied(tycon, _) => *tycon,
            _ => t,
        };
        let Type::TypeRef(prefix, name) = self.u.types().get(tycon) else { return None };
        match self.u.types().get(*prefix) {
            Type::Package(p) => Some(format!("{}.{name}", self.u.full_name(*p))),
            Type::PackageName(p) => Some(format!("{p}.{name}")),
            _ => None,
        }
    }
}
