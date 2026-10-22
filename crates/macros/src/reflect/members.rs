// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! The members, parameters, companions and children of symbols in `quotes.reflect`.

use super::{Answer, UNKNOWN, arg, numbers, symbol};
use crate::host::Host;
use tasty::tags::{CASEaccessor, ENUM};
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
        let params = self.u.symbol(s).decls.clone();
        if sizes.iter().sum::<usize>() != params.len() {
            return Err("Symbol.paramSymss of a method whose parameters the frontend lacks");
        }
        let head = [sizes.len() as i32].into_iter().chain(sizes.iter().map(|&n| n as i32));
        numbers(head.chain(params.iter().map(|p| p.0 as i32)).collect())
    }

    /// `methodMember(name)` or `methodMembers`: the methods of class `s` and its base classes but
    /// for constructors, a member overridden by one of an earlier class left out.
    fn method_members(&mut self, s: SymbolId, name: Option<&str>) -> Result<Vec<SymbolId>, &'static str> {
        let class = match self.u.is_module(s) {
            true => self.u.module_class(s),
            false => self.u.is_class(s).then_some(s),
        };
        let Some(class) = class else { return Ok(Vec::new()) };
        let mut found: Vec<(SymbolId, Option<String>)> = Vec::new();
        for base in self.u.linearization(class) {
            for d in self.u.symbol(base).decls.clone() {
                let sym = self.u.symbol(d);
                let method = sym.kind == Kind::Def && sym.name != "<init>" && sym.name != "$init$";
                if !method || name.is_some_and(|n| tasty::decode(n) != sym.name) {
                    continue;
                }
                if base != class && sym.flags.has(tasty::tags::PRIVATE) {
                    continue;
                }
                let sig = self.u.method_signature(d);
                let name = &self.u.symbol(d).name;
                if !found.iter().any(|&(f, ref g)| &self.u.symbol(f).name == name && *g == sig) {
                    found.push((d, sig));
                }
            }
        }
        Ok(found.into_iter().map(|(d, _)| d).collect())
    }

    /// `Symbol.children` of a sealed class of the unit: its direct subclasses and objects in the
    /// order of their definitions; an enum's cases.
    fn children(&mut self, s: SymbolId) -> Result<Vec<SymbolId>, &'static str> {
        if self.u.symbol(s).tree.is_some() {
            return Err("Symbol.children of a library class");
        }
        if self.u.symbol(s).flags.has(ENUM) {
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
