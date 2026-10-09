//! Whether an implicit definition may convert a class to a type with a given member or to a given
//! type, or offer an extension method of that name (dotty's `candidateKind` for a view prototype).

use super::Typer;
use super::did_you_mean::{KNOWN_JAVA, OBJECT_MEMBERS, is_jdk};
use super::implicits::conversions::Goal;
use super::java_members::java_members;
use super::widening::{is_numeric, widens};
use tasty::tags::{EXTENSION, GIVEN, IMPLICIT};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether member `m` of an object is an extension method named as `goal`'s member, or an
    /// implicit that may convert class `q` to fit `goal` or offer such an extension method.
    pub fn may_view(&mut self, m: SymbolId, q: SymbolId, goal: Goal) -> bool {
        let sym = self.u.symbol(m);
        if sym.flags.has(EXTENSION) {
            return matches!(goal, Goal::Member(name) if sym.name == name);
        }
        if !(sym.flags.has(IMPLICIT) || sym.flags.has(GIVEN)) || !matches!(sym.kind, Kind::Val | Kind::Def) {
            return false;
        }
        self.complete(m);
        let mut info = self.u.info(m);
        if let Type::Poly { result, .. } = self.u.types().get(info) {
            info = *result;
        }
        if let Type::Method { params, result, implicit: false, .. } = self.u.types().get(info).clone() {
            let [(_, param)] = params[..] else { return false };
            let result = self.strip_implicit_clauses(result);
            return self.may_accept(param, q, matches!(goal, Goal::Conform(_)))
                && match goal {
                    Goal::Member(name) => self.may_have(result, param, name),
                    Goal::Conform(to) => self.may_conform(result, param, to),
                };
        }
        let value = self.strip_implicit_clauses(info);
        let mut value = self.u.widen(value);
        while let Type::Refined(t, ..) | Type::Rec(_, t) | Type::Annotated(t, _) = *self.u.types().get(value)
        {
            value = t;
        }
        let Some(c) = self.u.class_symbol(value) else { return true };
        if let Goal::Member(name) = goal
            && self.class_members(c, name).iter().any(|&e| self.u.symbol(e).flags.has(EXTENSION))
        {
            return true;
        }
        let is_view = ["scala.Conversion", "scala.<:<"]
            .iter()
            .any(|v| self.resolve_full_name(v, true).is_some_and(|b| self.u.derives_from(c, b)));
        let identity = matches!(self.u.types().get(value), Type::Applied(_, args) if args.len() == 2 && args[0] == args[1]);
        is_view && !(identity && ["scala.<:<", "scala.=:="].contains(&&*self.u.full_name(c)))
    }

    fn strip_implicit_clauses(&self, mut t: TypeId) -> TypeId {
        while let Type::Method { result, implicit: true, .. } = self.u.types().get(t) {
            t = *result;
        }
        t
    }

    /// Whether class `q`, whose base classes the universe knows, may conform or widen numerically
    /// (only to a wider class if `widening`) to view parameter type `param`; not to a class of a
    /// package the universe lacks.
    fn may_accept(&mut self, param: TypeId, q: SymbolId, widening: bool) -> bool {
        let param = self.u.widen(param);
        let param = self.u.dealias(param);
        match *self.u.types().get(param) {
            Type::Or(a, b) => return self.may_accept(a, q, widening) || self.may_accept(b, q, widening),
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
        let Type::TypeRef(prefix, name) = self.u.types().get(tycon).clone() else { return None };
        match self.u.types().get(prefix).clone() {
            Type::Package(p) => Some(format!("{}.{name}", self.u.full_name(p))),
            Type::PackageName(p) => Some(format!("{p}.{name}")),
            _ => None,
        }
    }

    /// The class `t` refers to, applied or not; `None` for other types.
    pub fn direct_class(&mut self, t: TypeId) -> Option<SymbolId> {
        let tycon = match self.u.types().get(t) {
            Type::Applied(tycon, _) => *tycon,
            _ => t,
        };
        let c = match self.u.types().get(tycon).clone() {
            Type::Sym(_, c) => c,
            Type::TypeRef(p, n) => self.u.resolve(p, &n, true)?,
            _ => return None,
        };
        (self.u.symbol(c).kind == Kind::Class).then_some(c)
    }
}
