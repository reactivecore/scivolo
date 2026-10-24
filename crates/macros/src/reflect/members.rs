// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! The members, parameters, companions and children of symbols in `quotes.reflect`.

use super::{Answer, UNKNOWN, arg, numbers, symbol};
use crate::host::Host;
use tasty::tags::{CASE, CASEaccessor, ENUM, SEALED};
use tasty::{Kind, SymbolId, Type};
use typer::tpd::Tree as T;

/// Symbols as numbers, `-1` for none.
fn ids(syms: impl IntoIterator<Item = SymbolId>) -> Answer {
    numbers(syms.into_iter().map(|s| s.0 as i32).collect())
}

impl Host {
    /// Runs member operation `op` on `args` and `text`.
    pub fn run_member(&mut self, op: &str, args: &[i32], text: &str) -> Answer {
        let sym = || symbol(arg(args, 0)?);
        match op {
            "primaryConstructor" => {
                let s = sym()?;
                let ctor =
                    self.u.is_class(s).then(|| self.u.decl(s, "<init>").ok()?.first().copied()).flatten();
                numbers(vec![ctor.map_or(-1, |c| c.0 as i32)])
            }
            "paramSymss" => self.param_symss(sym()?),
            "caseFields" => {
                let s = sym()?;
                let decls = if self.u.is_class(s) { self.u.symbol(s).decls.clone() } else { Vec::new() };
                ids(decls.into_iter().filter(|&d| {
                    let d = self.u.symbol(d);
                    d.kind.is_term() && d.flags.has(CASEaccessor)
                }))
            }
            "companionClass" => {
                let s = sym()?;
                let class = match self.u.symbol(s).kind {
                    Kind::Package => None,
                    _ if self.u.is_module(s) => {
                        self.u.module_class(s).and_then(|c| self.u.companion_class(c))
                    }
                    _ if self.u.is_module_class(s) => self.u.companion_class(s),
                    Kind::Class => self.u.companion_module(s).and_then(|m| self.u.module_class(m)),
                    _ => None,
                };
                numbers(vec![class.map_or(-1, |c| c.0 as i32)])
            }
            "methodMembers" => {
                let s = sym()?;
                let methods = self.method_members(s, (!text.is_empty()).then_some(text))?;
                ids(methods)
            }
            "children" => {
                let s = sym()?;
                ids(self.children(s)?)
            }
            "requiredClass" => {
                let t = self.u.class_ref(text);
                match *self.u.types().get(t) {
                    Type::Sym(_, c) => numbers(vec![c.0 as i32]),
                    _ => Err("Symbol.requiredClass of a class the frontend lacks"),
                }
            }
            "requiredMethod" => {
                let (class, name) =
                    text.rsplit_once('.').ok_or("Symbol.requiredMethod of a name without a class")?;
                let c = self.u.class_ref(class);
                let c =
                    self.u.class_symbol(c).ok_or("Symbol.requiredMethod in a class the frontend lacks")?;
                let methods = self.u.member_where(c, name, |u, m| u.symbol(m).kind == Kind::Def);
                let m = methods.first().copied();
                numbers(vec![m.ok_or("Symbol.requiredMethod of a method the frontend lacks")?.0 as i32])
            }
            "requiredPackage" => match self.u.package(text) {
                Some(p) => numbers(vec![p.0 as i32]),
                None => Err("Symbol.requiredPackage of a package the frontend lacks"),
            },
            "symbolPos" => {
                let s = sym()?;
                let span = self.def_name_span(s).ok_or("Symbol.pos of a symbol defined outside the unit")?;
                self.located(span)
            }
            _ => Err(UNKNOWN),
        }
    }

    /// `Symbol.paramSymss`: the number of clauses, their sizes and the parameters, a method's
    /// clauses of type and value parameters, none for other symbols.
    fn param_symss(&mut self, s: SymbolId) -> Answer {
        if self.u.symbol(s).kind != Kind::Def {
            return numbers(vec![0]);
        }
        let mut sizes = Vec::new();
        let mut t = self.u.info(s);
        while let Type::Poly { params, result, .. } | Type::Method { params, result, .. } =
            self.u.types().get(t).clone()
        {
            sizes.push(params.len());
            t = result;
        }
        let mut params = self.u.symbol(s).decls.clone();
        params.retain(|&p| matches!(self.u.symbol(p).kind, Kind::Param | Kind::TypeParam));
        if sizes.iter().sum::<usize>() != params.len() {
            return Err("Symbol.paramSymss of a method whose parameters the frontend lacks");
        }
        let head = [sizes.len() as i32].into_iter().chain(sizes.iter().map(|&n| n as i32));
        numbers(head.chain(params.iter().map(|p| p.0 as i32)).collect())
    }

    /// `methodMember(name)` or `methodMembers`: the methods of class `s` and its base classes but
    /// for constructors, a member overridden by one of an earlier class, method or value, left out.
    fn method_members(&mut self, s: SymbolId, name: Option<&str>) -> Result<Vec<SymbolId>, &'static str> {
        let class = match self.u.is_module(s) {
            true => self.u.module_class(s),
            false => self.u.is_class(s).then_some(s),
        };
        let class = class.ok_or("Symbol.methodMembers of a symbol that is no class or object")?;
        let site = self.u.this_type(class);
        let mut found: Vec<SymbolId> = Vec::new();
        for base in self.u.linearization(class) {
            for d in self.u.symbol(base).decls.clone() {
                let sym = self.u.symbol(d);
                let member = sym.kind.is_term() && sym.name != "<init>" && sym.name != "$init$";
                if !member || name.is_some_and(|n| tasty::decode(n) != sym.name) {
                    continue;
                }
                if base != class && sym.flags.has(tasty::tags::PRIVATE) {
                    continue;
                }
                let earlier: Vec<SymbolId> = (found.iter().copied())
                    .filter(|&f| {
                        self.u.symbol(f).name == self.u.symbol(d).name && self.u.symbol(f).owner != Some(base)
                    })
                    .collect();
                if !earlier.into_iter().any(|f| self.same_member(site, f, d)) {
                    found.push(d);
                }
            }
        }
        Ok(found.into_iter().filter(|&d| self.u.symbol(d).kind == Kind::Def).collect())
    }

    /// Whether term members `a` and `b` of `site` are the same member, one overriding the other.
    fn same_member(&mut self, site: tasty::TypeId, a: SymbolId, b: SymbolId) -> bool {
        match (self.u.symbol(a).kind == Kind::Def, self.u.symbol(b).kind == Kind::Def) {
            (true, true) => self.u.matches(site, a, b) || self.is_nullary(a) && self.is_nullary(b),
            (true, false) => self.is_nullary(a),
            (false, true) => self.is_nullary(b),
            (false, false) => true,
        }
    }

    /// Whether method `m` takes no type parameters and no or one empty non-implicit clause.
    fn is_nullary(&mut self, m: SymbolId) -> bool {
        let info = self.u.info(m);
        let clauses = self.u.param_clauses(info);
        !matches!(self.u.types().get(info), Type::Poly { .. })
            && (clauses.is_empty() || clauses.len() == 1 && !clauses[0].0 && clauses[0].1.is_empty())
    }

    /// `Symbol.children` of a sealed class of the unit: its direct subclasses and objects in the
    /// order of their definitions; an enum's cases; none for other symbols.
    fn children(&mut self, s: SymbolId) -> Result<Vec<SymbolId>, &'static str> {
        let flags = self.u.symbol(s).flags;
        let enum_class = flags.has(ENUM) && !flags.has(CASE);
        if !self.u.is_class(s) || !enum_class && !flags.has(SEALED) {
            return Ok(Vec::new());
        }
        if self.u.symbol(s).tree.is_some() {
            return Err("Symbol.children of a library class");
        }
        if enum_class {
            return Ok(typer::sealed::enum_cases(&mut self.u, s));
        }
        let classes: Vec<(u32, SymbolId)> = (0..self.tpd.len())
            .map(typer::tpd::TNodeId::new)
            .filter(|&t| matches!(self.tpd.tree(t), T::TypeDef { .. }))
            .filter_map(|t| Some((self.tpd.span(t).start(), self.tpd.sym(t)?)))
            .filter(|&(_, c)| self.u.is_class(c))
            .collect();
        Ok(typer::sealed::direct_children(&mut self.u, s, &classes))
    }

    /// The span of the name in the definition of `s` in the unit's trees (dotty's `nameSpan`).
    fn def_name_span(&self, s: SymbolId) -> Option<syntax::ast::Span> {
        let def = (0..self.tpd.len()).map(typer::tpd::TNodeId::new).find(|&t| {
            self.tpd.sym(t) == Some(s)
                && matches!(self.tpd.tree(t), T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. })
        })?;
        let span = self.tpd.span(def);
        let name = self.u.symbol(s).name.trim_end_matches('$').len() as u32;
        span.exists().then(|| syntax::ast::Span::new(span.point(), span.point() + name, span.point()))
    }
}
