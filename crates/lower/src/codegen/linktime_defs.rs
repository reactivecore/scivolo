// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenStat.scala
//! The methods defining values resolved at link time (nscplugin's `genLinktimeResolved`,
//! `genLinktimeResolvedMethod`).

use super::Gen;
use super::method::Method;
use crate::symbols::has;
use crate::{Result, unsupported};
use nir::sig::{Scope, Sig};
use nir::{Attr, Body, Defn, DefnKind, LexicalScope, Op, PosId, Type, TypeId, Val, ValId};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::SymbolId;
use tasty::tags::FIELDaccessor;
use typer::tpd::{TNodeId, Tree};

const RESOLVED: &str = "scala.scalanative.unsafe.package$.resolved";

impl Gen<'_, '_> {
    /// Method `dd`, `sym`, resolved at link time, at `pos`: a call of the linker's property
    /// lookup, or its body's link-time conditions.
    pub fn gen_linktime_resolved(&mut self, dd: TNodeId, sym: SymbolId, pos: PosId) -> Result<Defn> {
        let span = self.l.tpd.span(dd);
        let Tree::DefDef { rhs: Some(rhs), .. } = *self.l.tpd.tree(dd) else {
            return unsupported(span, "an abstract method resolved at link time");
        };
        if has(self.l.u, sym, FIELDaccessor) {
            return unsupported(span, "a value resolved at link time");
        }
        let retty = self.result_type(sym);
        self.m = Method::new(Some(sym), 0, pos);
        let entry = self.fresh();
        self.label(entry, Vec::new());
        let value = match self.property(dd) {
            Some((name, false))
                if self.referenced(rhs).is_some_and(|s| self.l.u.full_name(s) == RESOLVED) =>
            {
                let string = self.class_type("java.lang.String", false);
                let nothing = self.ty(Type::Nothing);
                let linktime = self.pool.top("scala.scalanative.linktime");
                let sig = Sig::Method("resolveProperty".into(), vec![string, nothing], Scope::Public);
                let resolve = self.pool.member(linktime, &sig);
                let ty = self.ty(Type::Function(vec![string], retty));
                let ptr = self.val(Val::Global(resolve, ty));
                let name = self.str(&name);
                let args = vec![self.val(Val::String(name))];
                self.emit(Op::Call { ty, ptr, args })
            }
            Some((_, true)) => self.resolve(rhs, retty, span)?,
            _ => return unsupported(span, "a link-time property without `resolved` as its body"),
        };
        self.ret(value);
        let m = std::mem::replace(&mut self.m, Method::new(None, 0, pos));
        let scopes = vec![LexicalScope { id: 0, parent: 0, pos: self.no_pos }];
        let body = Body { insts: m.buf.insts, positions: m.buf.positions, local_names: Vec::new(), scopes };
        let name = self.method_name(sym);
        let ty = self.ty(Type::Function(Vec::new(), retty));
        let attrs = vec![Attr::AlwaysInline, Attr::LinktimeResolved];
        Ok(Defn { name, attrs, pos, kind: DefnKind::Define { ty, body } })
    }

    /// The value of the body `t` of a calculated link-time property of type `retty`, defined at
    /// `span`.
    fn resolve(&mut self, t: TNodeId, retty: TypeId, span: Span) -> Result<ValId> {
        match *self.l.tpd.tree(t) {
            Tree::Literal { .. } => self.gen_expr(t),
            Tree::If { cond, thenp, elsep, .. } => self.gen_linktime_if(retty, cond, thenp, elsep),
            Tree::Apply { .. } if *self.pool.ty(retty) == Type::Bool => {
                let tpe = self.l.tpd.tpe(t);
                let literal = |g: &mut Self, b| {
                    g.l.tpd.node(Tree::Literal { value: Constant::Boolean(b) }, span, Some(tpe), None)
                };
                let (yes, no) = (literal(self, true), literal(self, false));
                self.gen_linktime_if(retty, t, yes, no)
            }
            Tree::Block { stats, expr } if stats.is_empty() => match *self.l.tpd.tree(expr) {
                Tree::Typed { expr: inner, .. } if !matches!(self.l.tpd.tree(inner), Tree::Ident { .. }) => {
                    self.resolve(inner, retty, span)
                }
                Tree::Typed { .. } | Tree::Ident { .. } => {
                    unsupported(span, "a link-time property of a term")
                }
                _ => self.resolve(expr, retty, span),
            },
            _ => unsupported(self.l.tpd.span(t), "a link-time property of this expression"),
        }
    }
}
