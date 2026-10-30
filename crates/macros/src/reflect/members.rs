// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! The members, parameters, companions and children of symbols in `quotes.reflect`.

use super::{Answer, Fallible};
use crate::host::Host;
use tasty::tags::{CASE, CASEaccessor, ENUM, SEALED};
use tasty::{Kind, SymbolId, Type};
use typer::tpd::Tree as T;

impl Host {
    pub fn primary_constructor(&mut self, s: SymbolId) -> Option<SymbolId> {
        self.u.is_class(s).then(|| self.u.decl(s, "<init>").ok()?.first().copied()).flatten()
    }

    pub fn case_fields(&mut self, s: SymbolId) -> Vec<SymbolId> {
        let decls = if self.u.is_class(s) { self.u.symbol(s).decls.clone() } else { Vec::new() };
        let fields = decls.into_iter().filter(|&d| {
            let d = self.u.symbol(d);
            d.kind.is_term() && d.flags.has(CASEaccessor)
        });
        fields.collect()
    }

    pub fn companion_class(&mut self, s: SymbolId) -> Option<SymbolId> {
        match self.u.symbol(s).kind {
            Kind::Package => None,
            _ if self.u.is_module(s) => self.u.module_class(s).and_then(|c| self.u.companion_class(c)),
            _ if self.u.is_module_class(s) => self.u.companion_class(s),
            Kind::Class => self.u.companion_module(s).and_then(|m| self.u.module_class(m)),
            _ => None,
        }
    }

    /// `methodMember(name)` for a `name`, else `methodMembers`.
    pub fn method_members(&mut self, s: SymbolId, name: String) -> Fallible<Vec<SymbolId>> {
        self.methods_of(s, (!name.is_empty()).then_some(&name))
    }

    pub fn symbol_pos(&mut self, s: SymbolId) -> Answer {
        let span = self.def_name_span(s).ok_or("Symbol.pos of a symbol defined outside the unit")?;
        self.located(span)
    }

    /// `Symbol.paramSymss`: the number of clauses, their sizes and the parameters, a method's
    /// clauses of type and value parameters, none for other symbols; a secondary constructor
    /// shares the type parameters of the primary one.
    pub fn param_symss(&mut self, s: SymbolId) -> Fallible<Vec<i32>> {
        if self.u.symbol(s).kind != Kind::Def {
            return Ok(vec![0]);
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
        let constructor = self.u.symbol(s).name == "<init>";
        let primary =
            self.u.symbol(s).owner.filter(|_| constructor).and_then(|c| self.primary_constructor(c));
        let primary = primary.filter(|&p| p != s);
        if let Some(primary) = primary.filter(|_| sizes.iter().sum::<usize>() > params.len()) {
            let mut shared = self.u.symbol(primary).decls.clone();
            shared.retain(|&p| self.u.symbol(p).kind == Kind::TypeParam);
            params.splice(0..0, shared);
        }
        if sizes.iter().sum::<usize>() != params.len() {
            return Err("Symbol.paramSymss of a method whose parameters the frontend lacks");
        }
        let head = [sizes.len() as i32].into_iter().chain(sizes.iter().map(|&n| n as i32));
        Ok(head.chain(params.iter().map(|p| p.0 as i32)).collect())
    }

    /// The methods of class `s` and its base classes named `name` if given but for constructors,
    /// a member overridden by one of an earlier class, method or value, left out; those of the
    /// class of its type for a value.
    fn methods_of(&mut self, s: SymbolId, name: Option<&str>) -> Fallible<Vec<SymbolId>> {
        let class = match self.u.symbol(s).kind {
            _ if self.u.is_module(s) => self.u.module_class(s),
            _ if self.u.is_class(s) => Some(s),
            Kind::Val | Kind::Param => {
                let info = self.u.info(s);
                let info = self.u.widen(info);
                self.u.class_symbol(info).filter(|&c| self.u.is_class(c))
            }
            _ => None,
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

    /// `Symbol.children` of a sealed class: its direct subclasses and objects in the order of
    /// their definitions, of a library class as its TASTy names them; an enum's cases; none for
    /// other symbols.
    pub fn children(&mut self, s: SymbolId) -> Fallible<Vec<SymbolId>> {
        let flags = self.u.symbol(s).flags;
        let enum_class = flags.has(ENUM) && !flags.has(CASE);
        if !self.u.is_class(s) || !enum_class && !flags.has(SEALED) {
            return Ok(Vec::new());
        }
        if enum_class {
            return Ok(typer::sealed::enum_cases(&mut self.u, s));
        }
        if let Some(types) = self.u.decoded_children(s) {
            let symbol = |u: &mut tasty::Universe, t| match *u.types().get(t) {
                Type::Sym(_, c) => Some(c),
                Type::TypeRef(..) | Type::TermRef(..) => u.resolve_ref(t),
                _ => None,
            };
            let children = types.into_iter().map(|t| symbol(&mut self.u, t));
            return children.collect::<Option<_>>().ok_or("Symbol.children naming a type of another kind");
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
