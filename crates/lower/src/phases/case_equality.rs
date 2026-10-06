//! `equals` of case classes as dotty's `SyntheticMembers` writes it (`equalsBody`).

use super::casts::cast;
use crate::Lower;
use crate::definitions::{symbol, type_ref};
use crate::symbols::has;
use syntax::ast::Span;
use tasty::tags::{FINAL, SYNTHETIC};
use tasty::{Erased, Flags, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

impl Lower<'_> {
    /// `(this eq that) || that match { case x$0 @ (_: C) => this.a == x$0.a && ... && x$0.canEqual(this); case _ => false }`.
    /// Its local, owned by `class` until its method exists, goes to `locals`.
    pub fn equals_body(
        &mut self,
        class: SymbolId,
        accessors: &[SymbolId],
        that: TNodeId,
        locals: &mut Vec<SymbolId>,
    ) -> TNodeId {
        let cc = self.defs.case_classes;
        let class_type = type_ref(self.u, class);
        let that_as_class = self.u.define(symbol("x$0", Kind::Val, class, Flags::default().with(SYNTHETIC)));
        locals.push(that_as_class);
        self.u.set_info(that_as_class, class_type);
        let wildcard = self.wildcard(class_type);
        let tpt = self.type_tree(class_type, Span::NONE);
        let typed = self.tpd.node(Tree::Typed { expr: wildcard, tpt }, Span::NONE, Some(class_type), None);
        let name = self.term_name("x$0");
        let bind = Tree::Bind { name, body: typed };
        let pattern = self.tpd.node(bind, Span::NONE, Some(class_type), Some(that_as_class));
        let mut sorted = accessors.to_vec();
        sorted.sort_by_key(|&a| {
            let info = self.u.info(a);
            !matches!(self.u.erase(info), Erased::Prim(_))
        });
        let comparisons: Vec<TNodeId> = sorted
            .iter()
            .map(|&a| {
                let this = self.this(class, Span::NONE);
                let mine = self.select(this, a, Span::NONE);
                let other = self.ident(that_as_class, Span::NONE);
                let theirs = self.select(other, a, Span::NONE);
                self.equal(mine, theirs)
            })
            .collect();
        let mut rhs = match comparisons.is_empty() {
            true => self.boolean_literal(true),
            false => self.reduce_balanced(&comparisons, self.defs.patmat.and),
        };
        if !has(self.u, class, FINAL) {
            let other = self.ident(that_as_class, Span::NONE);
            let this = self.this(class, Span::NONE);
            let can_equal = self.call(other, cc.can_equal, &[this]);
            rhs = self.call(rhs, self.defs.patmat.and, &[can_equal]);
        }
        let matching = Tree::CaseDef { pat: pattern, guard: None, body: rhs };
        let matching = self.tpd.node(matching, Span::NONE, None, None);
        let any = type_ref(self.u, self.defs.any);
        let pat = self.wildcard(any);
        let no = self.boolean_literal(false);
        let default = self.tpd.node(Tree::CaseDef { pat, guard: None, body: no }, Span::NONE, None, None);
        let cases = self.tpd.list(&[matching, default]);
        let result = self.cases_lub(cases);
        let matched = self.tpd.node(Tree::Match { selector: that, cases }, Span::NONE, Some(result), None);
        let matched = self.positioned(matched);
        let this = self.this(class, Span::NONE);
        let that_object = self.copy_ident(that);
        let that_object = cast(self, that_object, &Erased::Class("java.lang.Object".into()));
        self.tpd.get_mut(that_object).span = Span::NONE;
        let same = self.call(this, cc.eq, &[that_object]);
        self.call(same, cc.or, &[matched])
    }

    /// `parts` joined by `op`: from the left, halves first beyond 100 parts (dotty's
    /// `reduceBalanced`).
    fn reduce_balanced(&mut self, parts: &[TNodeId], op: SymbolId) -> TNodeId {
        if parts.len() > 100 {
            let (leading, trailing) = parts.split_at(parts.len() / 2);
            let leading = self.reduce_balanced(leading, op);
            let trailing = self.reduce_balanced(trailing, op);
            return self.call(leading, op, &[trailing]);
        }
        let mut acc = parts[0];
        for &p in &parts[1..] {
            acc = self.call(acc, op, &[p]);
        }
        acc
    }
}
