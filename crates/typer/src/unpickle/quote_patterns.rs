// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/quoted/QuotePatterns.scala
//   compiler/src/scala/quoted/runtime/impl/QuoteMatcher.scala
//! Quote patterns as pickled for `QuoteMatching.ExprMatch` (dotty's `QuotePatterns`): a quote
//! whose body is a term hole `patternHole[T]`, after the type definitions of its bindings.

use super::{Refusal, Unpickler};
use crate::tpd::Tree as T;
use tasty::tags::{ANNOTATION, BLOCK, SYNTHETIC, TYPEAPPLY, TYPED, TYPEDEF};
use tasty::{Flags, Kind, NodeId, Symbol, SymbolId, Type, TypeId, View, view};

/// A quote pattern of a single typed term hole: the hole's type and the pattern's type
/// bindings, each with whether it is approximated from above (`@fromAbove`).
pub struct QuotePattern {
    pub hole: TypeId,
    pub bindings: Vec<(SymbolId, bool)>,
}

const SHAPE: Refusal = "a quote pattern other than a typed term hole";
const PATTERNS: &str = "scala.quoted.runtime.Patterns$";

impl Unpickler<'_> {
    /// The quote pattern quote body `body` is, if it is one, its spliced type definitions'
    /// symbols `spliced` standing for their types.
    pub fn pattern(
        &mut self,
        body: NodeId,
        spliced: &[(SymbolId, TypeId)],
    ) -> Result<Option<QuotePattern>, Refusal> {
        let (inner, bindings) = self.type_bindings(body)?;
        if bindings.is_empty() && !self.is_hole(inner) {
            return Ok(None);
        }
        let hole = self.hole_type(inner)?;
        let hole = self.u.map_type(hole, &mut |u, part| match *u.types().get(part) {
            Type::Sym(None, s) => spliced.iter().find(|&&(x, _)| x == s).map(|&(_, t)| t),
            _ => None,
        });
        Ok(Some(QuotePattern { hole, bindings }))
    }

    /// The type pattern quoted type body `body` is, if it is one: a block of type binding
    /// definitions and a type tree (`'[P]`, dotty's `QuoteMatcher` on type trees).
    pub fn type_pattern(
        &mut self,
        body: NodeId,
        spliced: &[(SymbolId, TypeId)],
    ) -> Result<Option<QuotePattern>, Refusal> {
        let (inner, bindings) = self.type_bindings(body)?;
        if bindings.is_empty() {
            return Ok(None);
        }
        let tpt = self.tpt(inner)?;
        let hole = self.tpd.tpe(tpt);
        self.check(hole)?;
        let hole = self.u.map_type(hole, &mut |u, part| match *u.types().get(part) {
            Type::Sym(None, s) => spliced.iter().find(|&&(x, _)| x == s).map(|&(_, t)| t),
            _ => None,
        });
        Ok(Some(QuotePattern { hole, bindings }))
    }

    /// The body of the block at `id` after its type binding definitions, and the bindings
    /// defined; `id` itself without bindings if it is not such a block (dotty's
    /// `instrumentTypeHoles`).
    fn type_bindings(&mut self, id: NodeId) -> Result<(NodeId, Vec<(SymbolId, bool)>), Refusal> {
        let k = self.kids(id);
        let all_bindings = k.len() >= 2
            && k[1..].iter().all(|&s| self.tag(s) == TYPEDEF)
            && k[1..].iter().all(|&s| self.annotation(s, "patternType"));
        if self.tag(id) != BLOCK || !all_bindings {
            return Ok((id, Vec::new()));
        }
        let mut bindings = Vec::new();
        for &def in &k[1..] {
            let View::TypeDef { rhs, .. } = view(self.ast(), def) else { return Err(SHAPE) };
            let sym = self.u.define(Symbol {
                name: self.name(def),
                kind: Kind::Type,
                owner: Some(self.owner),
                flags: Flags::default().with(SYNTHETIC),
                target: None,
                is_extern: false,
                derived: false,
                decls: Vec::new(),
                tree: Some((self.file, def)),
            });
            self.u.unenter(sym);
            let bounds = self.ty(rhs);
            self.u.set_info(sym, bounds);
            bindings.push((sym, self.annotation(def, "fromAbove")));
        }
        Ok((k[0], bindings))
    }

    /// Whether definition `def` has an annotation of class `Patterns.<name>`.
    fn annotation(&mut self, def: NodeId, name: &str) -> bool {
        let View::TypeDef { mods, .. } = view(self.ast(), def) else { return false };
        let annots: Vec<NodeId> = mods.iter().copied().filter(|&m| self.tag(m) == ANNOTATION).collect();
        annots.into_iter().any(|m| {
            let tpe = self.ty(self.kids(m)[0]);
            self.u.class_symbol(tpe).is_some_and(|c| self.is_patterns_member(c, name))
        })
    }

    /// Whether the tree at `id`, possibly ascribed, applies a method named `patternHole` to a
    /// type.
    fn is_hole(&self, id: NodeId) -> bool {
        let id = if self.tag(id) == TYPED { self.kids(id)[0] } else { id };
        self.tag(id) == TYPEAPPLY && self.name(self.ast().deref(self.kids(id)[0])) == "patternHole"
    }

    /// The type `T` of the term hole `patternHole[T]` at `id`, possibly ascribed.
    fn hole_type(&mut self, id: NodeId) -> Result<TypeId, Refusal> {
        let id = if self.tag(id) == TYPED { self.kids(id)[0] } else { id };
        let t = self.term(id)?;
        let T::TypeApply { fun, args } = *self.tpd.tree(t) else { return Err(SHAPE) };
        let [tpt] = self.tpd.items(args)[..] else { return Err(SHAPE) };
        let hole = self.tpd.sym(fun).is_some_and(|s| self.is_patterns_member(s, "patternHole"));
        hole.then(|| self.tpd.tpe(tpt)).ok_or(SHAPE)
    }

    /// Whether `s` is member `name` of object `scala.quoted.runtime.Patterns`.
    fn is_patterns_member(&self, s: SymbolId, name: &str) -> bool {
        let owner = self.u.symbol(s).owner.map(|o| self.u.full_name(o));
        self.u.symbol(s).name == name && owner.as_deref() == Some(PATTERNS)
    }
}
