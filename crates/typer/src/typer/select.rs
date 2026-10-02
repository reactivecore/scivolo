//! Selections `qual.name` and the members of a type (dotty's `typedSelect`, `Type.member`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, TName};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The members named `name` of type `t`.
    pub fn type_members(&mut self, t: TypeId, name: &str) -> Vec<SymbolId> {
        match self.u.types().get(t).clone() {
            Type::Package(p) => self.u.decl(p, name).unwrap_or_default(),
            Type::This(c) => self.type_members(c, name),
            Type::Constant(..) | Type::Str(_) | Type::TermRef(..) => {
                let widened = self.u.widen(t);
                self.type_members(widened, name)
            }
            Type::Sym(_, s) => match self.u.symbol(s).kind {
                Kind::Package => self.u.decl(s, name).unwrap_or_default(),
                Kind::Class => self.class_members(s, name),
                Kind::Val if self.u.symbol(s).flags.has(tasty::tags::OBJECT) => {
                    self.u.module_class(s).map_or_else(Vec::new, |c| self.class_members(c, name))
                }
                _ => {
                    self.complete(s);
                    let info = self.u.info(s);
                    self.type_members(info, name)
                }
            },
            _ => match self.u.class_symbol(t) {
                Some(c) => self.class_members(c, name),
                None => Vec::new(),
            },
        }
    }

    /// The members named `name` of class `c`, its base classes' included.
    pub fn class_members(&mut self, c: SymbolId, name: &str) -> Vec<SymbolId> {
        self.complete(c);
        let classes = self.u.linearization(c);
        for &base in classes.iter().skip(1) {
            self.complete(base);
        }
        self.u.member(c, name)
    }

    /// Whether term `s` denotes the same value each time: not a variable or a by-name
    /// parameter, not a method unless a stable one (dotty's `isStableMember`).
    pub fn is_stable_member(&mut self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        match sym.kind {
            Kind::Param => {
                let info = self.u.info(s);
                !matches!(self.u.types().get(info), Type::ByName(_))
            }
            Kind::Val => !sym.flags.has(tasty::tags::MUTABLE),
            Kind::Def => sym.flags.has(tasty::tags::STABLE),
            _ => true,
        }
    }

    /// `t` widened while it is a by-name type or a reference to an unstable term (dotty's
    /// `widenIfUnstable`).
    fn widen_if_unstable(&mut self, t: TypeId) -> TypeId {
        match self.u.types().get(t).clone() {
            Type::ByName(r) => self.widen_if_unstable(r),
            Type::Sym(_, s) if !self.is_type_symbol(s) && !self.is_stable_member(s) => {
                match self.method_info(t) {
                    Some(info) if info != t => self.widen_if_unstable(info),
                    _ => t,
                }
            }
            _ => t,
        }
    }

    pub fn typed_select(&mut self, id: NodeId, qualifier: NodeId, name: TName, pt: Pt, ctx: Ctx) -> TNodeId {
        let qual = self.typed(qualifier, Pt::Any, ctx);
        let qual_tpe = self.tpd.tpe(qual);
        let qual_tpe = self.widen_if_unstable(qual_tpe);
        let text = self.text(name.name).to_owned();
        let syms: Vec<SymbolId> = self
            .type_members(qual_tpe, &text)
            .into_iter()
            .filter(|&s| self.is_type_symbol(s) == name.is_type)
            .collect();
        if syms.is_empty() {
            return self.unsupported(id, "member not found: extension or conversion");
        }
        let sym = match self.choose(&syms, Some(qual_tpe), pt) {
            Ok(sym) => sym,
            Err(what) => return self.unsupported(id, what),
        };
        if let Err(what) = self.check_reference(sym, Some(qual_tpe), ctx) {
            return self.unsupported(id, what);
        }
        let flags = self.u.symbol(sym).flags;
        let protected = flags.has(tasty::tags::PROTECTED) || flags.has(tasty::tags::PROTECTEDqualified);
        if protected && !matches!(self.u.types().get(qual_tpe), Type::This(_)) {
            return self.unsupported(id, "protected member through a path");
        }
        let projection = self.tpd.sym(qual).is_some_and(|q| self.is_type_symbol(q));
        if projection && self.u.symbol(sym).kind == Kind::Type {
            self.complete(sym);
            let info = self.u.info(sym);
            if matches!(self.u.types().get(info), Type::Alias(_)) {
                return self.unsupported(id, "projection of a type alias");
            }
        }
        let tpe = self.ref_type(Some(qual_tpe), sym);
        let span = self.ast.span(id);
        let select = self.tpd.node(T::Select { qualifier: qual, name }, span, Some(tpe), Some(sym));
        self.fold_select(select, &text);
        select
    }
}
