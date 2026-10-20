// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Decorators.scala
//   compiler/src/dotty/tools/dotc/transform/SyntheticMembers.scala
//! `equals` of case classes as dotty's `SyntheticMembers` writes it (`equalsBody`).

use super::casts::cast;
use crate::Lower;
use crate::definitions::{CaseClasses, primitive_class_name, symbol, type_ref};
use crate::symbols::has;
use syntax::ast::Span;
use tasty::tags::{FINAL, SYNTHETIC};
use tasty::{Erased, Flags, Kind, SymbolId};
use typer::tpd::{MatchKind, TNodeId, Tree};

impl Lower<'_> {
    /// `(this eq that) || that match { case x$0 @ (_: C) => this.a == x$0.a && ... && x$0.canEqual(this); case _ => false }`,
    /// calling `can_equal`, the `canEqual` the class has, if any; only the match for a value
    /// class. Its local, owned by `class` until its method exists, goes to `locals`.
    pub fn equals_body(
        &mut self,
        class: SymbolId,
        accessors: &[SymbolId],
        that: TNodeId,
        can_equal: Option<SymbolId>,
        locals: &mut Vec<SymbolId>,
        cc: CaseClasses,
    ) -> TNodeId {
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
            let symbol = match *self.u.types().get(info) {
                tasty::Type::Sym(_, s) => Some(s),
                _ => None,
            };
            let primitive = symbol.is_some_and(|s| {
                let name = self.u.full_name(s);
                name == "scala.Unit"
                    || "zcbsijfd".chars().any(|p| primitive_class_name(p).as_deref() == Some(name.as_str()))
            });
            !primitive
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
            false => self.reduce_balanced(&comparisons, self.defs.and),
        };
        if !has(self.u, class, FINAL) || can_equal.is_some() {
            let other = self.ident(that_as_class, Span::NONE);
            let this = self.this(class, Span::NONE);
            let can_equal = self.call(other, can_equal.unwrap_or(cc.can_equal), &[this]);
            rhs = self.call(rhs, self.defs.and, &[can_equal]);
        }
        let matching = Tree::CaseDef { pat: pattern, guard: None, body: rhs };
        let matching = self.tpd.node(matching, Span::NONE, None, None);
        let any = type_ref(self.u, self.defs.any);
        let pat = self.wildcard(any);
        let no = self.boolean_literal(false);
        let default = self.tpd.node(Tree::CaseDef { pat, guard: None, body: no }, Span::NONE, None, None);
        let cases = self.tpd.list(&[matching, default]);
        let result = self.cases_lub(cases);
        let matched = self.tpd.node(
            Tree::Match { selector: Some(that), cases, kind: MatchKind::Plain },
            Span::NONE,
            Some(result),
            None,
        );
        let matched = self.positioned(matched);
        if self.u.underlying(class).is_some() {
            return matched;
        }
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
