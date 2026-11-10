// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Splicing.scala
//   compiler/src/dotty/tools/dotc/staging/CrossStageSafety.scala
//! References under a splice to types of its quote: an alias dealiased, an abstract type `T` in
//! a nested quote a tag of that quote, the `Type[T]` parameter `Type$N` its hole binds.

use super::splicing::Splicer;
use super::type_quotes::TypeQuote;
use crate::Lower;
use rustc_hash::FxHashSet;
use syntax::ast::Span;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};
use typer::tpd::{TNodeId, Tree};

impl Splicer<'_> {
    /// Whether `t` at quote level `level` is a type tree that may refer to a type of the quote: a
    /// reference to one becomes a `TypeTree` at `at` of its type dealiased, and a type of a nested
    /// quote gets its tag.
    pub fn captured_type(&mut self, l: &mut Lower, (t, at): (TNodeId, Span), level: i32) -> bool {
        match *l.tpd.tree(t) {
            Tree::Ident { name } if name.is_type => {
                let Some(s) = l.tpd.sym(t).filter(|s| self.defined.contains(s)) else { return false };
                let tpe = l.tpd.tpe(t);
                let abstract_type = is_abstract(l.u, s);
                if abstract_type && level >= 1 {
                    self.tag(l, s);
                }
                let span = if abstract_type { at } else { l.tpd.span(t) };
                let dealiased = l.u.dealias(tpe);
                let node = l.tpd.get_mut(t);
                (node.tree, node.sym, node.tpe, node.span) =
                    (Tree::TypeTree { inferred: false }, None, Some(dealiased), span);
                self.retyped += 1;
                true
            }
            Tree::TypeTree { .. } if level >= 1 => {
                let tpe = l.tpd.tpe(t);
                match l.u.sym_ref(tpe) {
                    Some((None, s)) if self.defined.contains(&s) && is_abstract(l.u, s) => self.tag(l, s),
                    _ if refers_to(l.u, self.defined, tpe) => {
                        self.captures.untranslated.get_or_insert(t);
                    }
                    _ => {}
                }
                true
            }
            Tree::TypeTree { .. } => true,
            _ => false,
        }
    }

    /// Makes the type of applied type tree `t` that of its parts if a part was retyped since
    /// `before`.
    pub fn retype_applied(&mut self, l: &mut Lower, t: TNodeId, before: usize) {
        let Tree::AppliedTypeTree { tpt, args } = *l.tpd.tree(t) else { return };
        if self.retyped == before {
            return;
        }
        let args: Vec<_> = l.tpd.items(args).iter().map(|&a| l.tpd.tpe(a)).collect();
        let applied = l.u.intern_applied(l.tpd.tpe(tpt), &args);
        let applied = l.u.beta_reduced(applied);
        l.tpd.get_mut(t).tpe = Some(applied);
        self.retyped += 1;
    }

    /// Transforms type quote `q` at level 0 in `owner`: a body whose type refers to abstract types
    /// of the quote becomes a `TypeTree` and the quote gets their tags (dotty's
    /// `capturedPartTypes`); another body is walked at level 1.
    pub fn captured_type_quote(&mut self, l: &mut Lower, q: &TypeQuote, owner: SymbolId) {
        let mut found = Vec::new();
        let tpe = l.tpd.tpe(q.body);
        l.u.map_type(tpe, &mut |u, x| {
            if let Some((None, s)) = u.sym_ref(x)
                && self.defined.contains(&s)
                && is_abstract(u, s)
                && !found.contains(&s)
            {
                found.push(s);
            }
            None
        });
        if found.is_empty() {
            self.walk(l, (q.body, Span::NONE), 1, owner);
            return;
        }
        l.spliced_type_quotes.insert(q.of);
        let outer = self.quote.replace(q.of);
        for s in found {
            self.tag(l, s);
        }
        self.quote = outer;
        let node = l.tpd.get_mut(q.body);
        (node.tree, node.sym, node.span) = (Tree::TypeTree { inferred: false }, None, node.span.start_pos());
    }

    /// Records the `Type[T]` parameter of abstract type `s` of the quote as the tag of `s` in the
    /// nested quote being walked.
    fn tag(&mut self, l: &mut Lower, s: SymbolId) {
        let Some(quote) = self.quote else { return };
        let tpe = l.u.intern_type(Type::Sym(None, s));
        let arg = l.tpd.node(Tree::TypeTree { inferred: false }, Span::NONE, Some(tpe), None);
        let param = self.bind(l, arg, s, ("Type", "scala.quoted.Type", tpe));
        let tag: TypeId = l.u.intern_type(Type::Sym(None, param));
        let mut tags = l.tpd.quote_tags(quote).to_vec();
        if !tags.iter().any(|&(t, _)| t == s) {
            tags.push((s, tag));
            l.tpd.set_quote_tags(quote, tags);
        }
    }
}

/// Whether a part of `t`, aliases followed, refers to one of the definitions `defined`.
fn refers_to(u: &mut Universe, defined: &FxHashSet<SymbolId>, t: TypeId) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, x| match u.sym_of(x) {
        Some(s) if defined.contains(&s) => {
            match u.referenced_alias(x) {
                Some(a) => found |= refers_to(u, defined, a),
                None => found = true,
            }
            Some(x)
        }
        _ => None,
    });
    found
}

/// Whether `s` is a type or type parameter that is not an alias.
fn is_abstract(u: &mut Universe, s: SymbolId) -> bool {
    matches!(u.symbol(s).kind, Kind::Type | Kind::TypeParam) && u.alias_of(s).is_none()
}
