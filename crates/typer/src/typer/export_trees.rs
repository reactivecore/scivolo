// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The typed trees of export clauses and of their forwarders (dotty's `typedExport` and the
//! definitions `exportForwarders` builds).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::namer::exports::Forwarder;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The typed export clause `id` in class context `ctx`, followed by its forwarders.
    pub fn typed_export(&mut self, id: NodeId, ctx: Ctx) -> Vec<TNodeId> {
        let Tree::Export { expr, selectors, .. } = *self.ast.tree(id) else { unreachable!() };
        let Some(exported) = self.exports.get(&id).cloned() else {
            return vec![self.unsupported(id, "export clause outside a class")];
        };
        let path = self.export_path(expr, exported.path_method, ctx);
        let items = self.ast.items(selectors).to_vec();
        let typed: Vec<TNodeId> = items.into_iter().map(|s| self.untyped_selector(s, None)).collect();
        let selectors = self.tpd.list(&typed);
        let span = self.ast.span(id);
        let mut out =
            vec![self.tpd.node(T::Import { expr: path, selectors, export: true }, span, None, None)];
        for f in exported.forwarders {
            out.push(self.forwarder_def(f, expr, exported.path_method, ctx));
        }
        out
    }

    /// Path `expr` of an export typed in `ctx`: a reference to `path_method` in an extension.
    fn export_path(&mut self, expr: NodeId, path_method: Option<SymbolId>, ctx: Ctx) -> TNodeId {
        let Some(m) = path_method else { return self.typed(expr, Pt::Any, ctx.with_mode(Mode::QUALIFIER)) };
        let tpe = crate::defn::type_ref(self.u, m);
        let span = self.ast.span(expr);
        self.build().ident(m, tpe, span)
    }

    /// The definition of forwarder `f` whose body selects from path `expr`, applied to the
    /// forwarder's leading parameters if it refers to `path_method`.
    fn forwarder_def(
        &mut self,
        f: Forwarder,
        expr: NodeId,
        path_method: Option<SymbolId>,
        ctx: Ctx,
    ) -> TNodeId {
        let at = Span::at(f.span.point());
        let name = TName { name: self.name(&self.u.symbol(f.sym).name.clone()), is_type: false };
        let info = self.u.info(f.sym);
        if self.u.is_type_symbol(f.sym) {
            let rhs = self.type_tree(info, at, false);
            let name = TName { is_type: true, ..name };
            return self.tpd.node(T::TypeDef { name, rhs }, f.span, None, Some(f.sym));
        }
        let mut fun = self.export_path(expr, path_method, ctx.with_owner(f.sym));
        let path_def = path_method.and_then(|m| self.syms.def_trees.get(&m).copied());
        let mut path_clauses = path_def.map_or(0, |d| self.def_clauses(d).len());
        let path_span = self.ast.span(expr);
        if path_clauses == 0 {
            fun = self.build().select(fun, f.target, at);
        }
        let mut params = self.u.symbol(f.sym).decls.clone().into_iter();
        let mut clauses = Vec::new();
        let mut tpe = info;
        loop {
            let (binder, names, poly, result) = match self.u.types().get(tpe).clone() {
                Type::Poly { binder, params, result } => (binder, params, true, result),
                Type::Method { binder, params, result, .. } => (binder, params, false, result),
                _ => break,
            };
            let syms: Vec<_> = params.by_ref().take(names.len()).collect();
            let names: Vec<String> = names.into_iter().map(|(n, _)| n).collect();
            let refs: Vec<TypeId> = syms.iter().map(|&s| self.u.intern_type(Type::Sym(None, s))).collect();
            tpe = self.u.subst(result, binder, &names, &refs);
            let (defs, args) = self.forwarder_params(&syms, poly, at, path_clauses > 0, path_span);
            let applied = if path_clauses > 0 { self.applied_path_type(fun, &refs) } else { tpe };
            let span = if path_clauses > 0 { path_span.to_synthetic() } else { at };
            fun = match poly {
                true => self.build().type_apply(fun, &args, applied, span),
                false => self.build().apply(fun, &args, applied, span),
            };
            clauses.push(defs);
            if path_clauses == 1 {
                fun = self.build().select(fun, f.target, at);
            }
            path_clauses = path_clauses.saturating_sub(1);
        }
        let tpt = self.type_tree(tpe, at, false);
        let clauses: Vec<&[TNodeId]> = clauses.iter().map(Vec::as_slice).collect();
        self.build().def_def(f.sym, &clauses, tpt, Some(fun), f.span)
    }

    /// The definitions of forwarder parameters `syms`, type parameters if `poly`, and the
    /// arguments referring to them, at the end of `path_span` for a path method's clause.
    fn forwarder_params(
        &mut self,
        syms: &[SymbolId],
        poly: bool,
        at: Span,
        of_path: bool,
        path_span: Span,
    ) -> (Vec<TNodeId>, Vec<TNodeId>) {
        let arg_at = if of_path { Span::at(path_span.end()) } else { at };
        let mut defs = Vec::new();
        let mut args = Vec::new();
        for &s in syms {
            let pname = self.name(&self.u.symbol(s).name.clone());
            let r = self.u.intern_type(Type::Sym(None, s));
            if poly {
                let bounds = self.u.info(s);
                let rhs = self.type_tree(bounds, at, false);
                let pname = TName { name: pname, is_type: true };
                defs.push(self.tpd.node(T::TypeDef { name: pname, rhs }, at, None, Some(s)));
                args.push(self.type_tree(r, arg_at, false));
                continue;
            }
            let ptype = self.method_param_type(s);
            let tpt = self.type_tree(ptype, at, false);
            let pname = TName { name: pname, is_type: false };
            defs.push(self.tpd.node(T::ValDef { name: pname, tpt, rhs: None }, at, None, Some(s)));
            let arg = self.build().ident(s, r, arg_at);
            args.push(match self.syms.repeated_params.contains_key(&s) {
                true => {
                    let tpt = self.type_tree(ptype, arg_at, true);
                    self.tpd.node(T::Typed { expr: arg, tpt }, arg_at, Some(ptype), None)
                }
                false => arg,
            });
        }
        (defs, args)
    }

    /// The type of path method reference `fun` applied to parameters `refs`.
    fn applied_path_type(&mut self, fun: TNodeId, refs: &[TypeId]) -> TypeId {
        let fun_tpe = self.tpd.tpe(fun);
        let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
        let (Type::Poly { binder, params, result } | Type::Method { binder, params, result, .. }) =
            self.u.types().get(info).clone()
        else {
            return info;
        };
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
        self.u.subst(result, binder, &names, refs)
    }
}
