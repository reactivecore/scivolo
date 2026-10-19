//! Quote patterns as pickled for `QuoteMatching.ExprMatch` (dotty's `QuotePatterns`): a quote
//! whose body is a term hole `patternHole[T]`, after the type definitions of its bindings.

use super::quotes::unpickler;
use super::{Refusal, Unpickler};
use crate::tpd::{Tpd, Tree as T};
use syntax::names::Names;
use tasty::tags::{ANNOTATION, BLOCK, SYNTHETIC, TYPED, TYPEDEF};
use tasty::{Flags, Kind, NodeId, Symbol, SymbolId, Type, TypeId, Universe, View, view};

/// A quote pattern of a single typed term hole: the hole's type and the pattern's type
/// bindings, each with whether it is approximated from above (`@fromAbove`).
pub struct QuotePattern {
    pub hole: TypeId,
    pub bindings: Vec<(SymbolId, bool)>,
}

const SHAPE: Refusal = "a quote pattern other than a typed term hole";

/// The quote pattern whose TASTy is `bytes`, owned by `owner`, its type holes filled with
/// `types`.
pub fn quote_pattern(
    lent: (&mut Universe, &mut Tpd, &mut Names),
    bytes: Vec<u8>,
    owner: SymbolId,
    types: &[TypeId],
) -> Result<QuotePattern, Refusal> {
    let (mut p, root) = unpickler(lent, bytes, owner)?;
    let View::Inlined { expr, bindings: [], .. } = view(p.ast(), root) else { return Err(SHAPE) };
    let (body, spliced) = p.spliced_types(expr, types)?;
    let (body, bindings) = p.type_bindings(body)?;
    let hole = p.hole_type(body)?;
    let hole = p.u.map_type(hole, &mut |u, part| match *u.types().get(part) {
        Type::Sym(None, s) => spliced.iter().find(|&&(x, _)| x == s).map(|&(_, t)| t),
        _ => None,
    });
    Ok(QuotePattern { hole, bindings })
}

impl Unpickler<'_> {
    /// The body of the block at `id` after its type binding definitions, and the bindings
    /// defined; `id` itself without bindings if it is not such a block.
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
            self.u.class_symbol(tpe).is_some_and(|c| self.u.symbol(c).name == name)
        })
    }

    /// The type `T` of the term hole `patternHole[T]` at `id`, possibly ascribed.
    fn hole_type(&mut self, id: NodeId) -> Result<TypeId, Refusal> {
        let id = if self.tag(id) == TYPED { self.kids(id)[0] } else { id };
        let t = self.term(id)?;
        let T::TypeApply { fun, args } = *self.tpd.tree(t) else { return Err(SHAPE) };
        let [tpt] = self.tpd.items(args)[..] else { return Err(SHAPE) };
        let hole = self.tpd.sym(fun).is_some_and(|s| {
            let owner = self.u.symbol(s).owner.map(|o| self.u.full_name(o));
            self.u.symbol(s).name == "patternHole"
                && owner.is_some_and(|o| o.contains("quoted.runtime.Patterns"))
        });
        hole.then(|| self.tpd.tpe(tpt)).ok_or(SHAPE)
    }
}
