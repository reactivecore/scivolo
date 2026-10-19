// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//! `new C[Ts](...)` for a class, and references to its parameters (dotty's `creatorExpr`,
//! `makeNew` and `refOfDef`).

use super::Desugar;
use super::class_parts::ClassParts;
use crate::ast::flags::{self, FlagSet};
use crate::ast::{Attrs, NodeId, TName, Tree};

impl Desugar<'_> {
    /// `new C[Ts](params)...`, made once per class.
    pub fn creator_expr(&mut self, c: &mut ClassParts) -> NodeId {
        if let Some(t) = c.creator {
            return t;
        }
        let first_is = |d: &Self, ps: &[NodeId], f: FlagSet| {
            ps.first().and_then(|&p| d.ast.mods_of(p)).is_some_and(|m| d.ast.modifiers(m).is_one_of(f))
        };
        let mut vparamss = c.constr_vparamss.clone();
        if vparamss.first().is_some_and(|ps| first_is(self, ps, flags::IMPLICIT)) {
            vparamss.insert(0, Vec::new());
        } else if !vparamss.is_empty() && vparamss.iter().all(|ps| first_is(self, ps, flags::GIVEN)) {
            vparamss.push(Vec::new());
        }
        let mut nu = self.make_new(c.class_type_ref);
        for vparams in &vparamss {
            let args: Vec<NodeId> = vparams.iter().map(|&p| self.ref_of_def(p)).collect();
            nu = self.apply(nu, &args);
            let evidence = vparams.first().is_some_and(|&p| {
                self.ast.attrs(p).contains(Attrs::EVIDENCE)
                    && self.text(self.name_of_def(p)).starts_with("evidence$")
            });
            if first_is(self, vparams, flags::GIVEN_OR_IMPLICIT) || evidence {
                self.ast.add_attrs(nu, Attrs::USING_APPLY);
            }
        }
        let t = if matches!(self.ast.tree(nu), Tree::Apply { .. }) { nu } else { self.apply(nu, &[]) };
        c.creator = Some(t);
        t
    }

    /// `new tpt` before its arguments: `Select(New(tycon), <init>)` with type arguments.
    pub fn make_new(&mut self, tpt: NodeId) -> NodeId {
        let (tycon, targs) = match *self.ast.tree(tpt) {
            Tree::AppliedTypeTree { tpt, args } => (tpt, Some(args)),
            _ => (tpt, None),
        };
        let new = self.ast.node(Tree::New { tpt: tycon });
        let init = self.term("<init>");
        let nu = self.select(new, init);
        match targs {
            Some(args) => self.ast.node(Tree::TypeApply { fun: nu, args }),
            None => nu,
        }
    }

    /// A reference to parameter `vparam`, `x: _*` for a repeated one.
    pub fn ref_of_def(&mut self, vparam: NodeId) -> NodeId {
        let name = self.name_of_def(vparam);
        let id = self.ident(name);
        let Tree::ValDef { tpt, .. } = *self.ast.tree(vparam) else { return id };
        if !self.is_star(tpt) {
            return id;
        }
        let star = self.type_name("_*");
        let tpt = self.ident(star);
        self.ast.node(Tree::Typed { expr: id, tpt })
    }

    /// Whether `tpt` is `T*` or `=> T*`.
    pub fn is_repeated(&self, tpt: NodeId) -> bool {
        match *self.ast.tree(self.unsplice(tpt)) {
            Tree::ByNameTypeTree { result } => self.is_star(result),
            _ => self.is_star(tpt),
        }
    }

    fn is_star(&self, tpt: NodeId) -> bool {
        matches!(*self.ast.tree(tpt), Tree::PostfixOp { op, .. }
            if matches!(*self.ast.tree(op), Tree::Ident { name } if name.is_type && self.text(name) == "*"))
    }

    pub fn name_of_def(&self, def: NodeId) -> TName {
        self.def_name(def).expect("a definition")
    }
}
