// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/quoted/PickledQuotes.scala
//! Pickled quotes (dotty's `PickledQuotes.unpickleTerm`, `unpickleTypeTree`, `spliceTypes`): the
//! TASTy of a quote read into typed trees owned by the splice, each spliced type definition a
//! symbol aliasing the type it stands for, each term hole a placeholder to fill.

use super::quote_patterns::QuotePattern;
use super::{Refusal, Unpickler};
use crate::tpd::{TNodeId, Tpd, Tree as T};
use syntax::ast::TName;
use syntax::names::Names;
use tasty::tags::{ANNOTATION, BLOCK, HOLE, TYPEDEF};
use tasty::{Flags, Kind, NodeId, Symbol, SymbolId, Type, TypeId, Universe, View, view};

/// A term hole of a quote: its placeholder tree, the index its pickle gives it and its arguments.
pub struct QuoteHole {
    pub node: TNodeId,
    pub index: u32,
    pub args: Vec<TNodeId>,
}

/// An unpickled quote: its tree, the spliced type definitions with the types they stand for,
/// and its term holes.
pub struct Quote {
    pub root: TNodeId,
    pub spliced: Vec<(SymbolId, TypeId)>,
    pub holes: Vec<QuoteHole>,
}

/// An unpickled quote or quote pattern.
pub enum Unpickled {
    Quote(Quote),
    Pattern(QuotePattern),
}

/// The quote or quote pattern whose TASTy is `bytes`, owned by `owner`, its type holes filled
/// with `types`.
pub fn quote(
    (u, tpd, names): (&mut Universe, &mut Tpd, &mut Names),
    bytes: Vec<u8>,
    owner: SymbolId,
    types: &[TypeId],
) -> Result<Unpickled, Refusal> {
    let (mut p, root) = unpickler((u, tpd, names), bytes, owner)?;
    let (expr, call) = match view(p.ast(), root) {
        View::Inlined { expr, call, bindings: [] } => (expr, call),
        _ => return Err("a quote of another shape"),
    };
    let (body, spliced) = p.spliced_types(expr, types)?;
    if let Some(pattern) = p.pattern(body, &spliced)? {
        return Ok(Unpickled::Pattern(pattern));
    }
    let call = call.map(|c| p.tpt(c)).transpose()?;
    let expansion = p.term(body)?;
    let tpe = p.tpd.tpe(expansion);
    let bindings = p.tpd.list(&[]);
    let root_tree = p.bare(T::Inlined { call, bindings, expansion }, Some(tpe), None);
    let root = p.positioned(root, root_tree);
    Ok(Unpickled::Quote(Quote { root, spliced, holes: p.holes.take().unwrap_or_default() }))
}

/// The type whose TASTy is `bytes`, its type holes filled with `types`; `Err` for a type pattern.
pub fn quoted_type(
    (u, tpd, names): (&mut Universe, &mut Tpd, &mut Names),
    bytes: Vec<u8>,
    owner: SymbolId,
    types: &[TypeId],
) -> Result<Result<TypeId, QuotePattern>, Refusal> {
    let (mut p, root) = unpickler((u, tpd, names), bytes, owner)?;
    let (body, spliced) = p.spliced_types(root, types)?;
    if let Some(pattern) = p.type_pattern(body, &spliced)? {
        return Ok(Err(pattern));
    }
    let tpt = p.tpt(body)?;
    let tpe = p.tpd.tpe(tpt);
    p.check(tpe)?;
    Ok(Ok(p.u.map_type(tpe, &mut |u, part| match *u.types().get(part) {
        Type::Sym(None, s) => spliced.iter().find(|&&(x, _)| x == s).map(|&(_, t)| t),
        _ => None,
    })))
}

fn unpickler<'a>(
    (u, tpd, names): (&'a mut Universe, &'a mut Tpd, &'a mut Names),
    bytes: Vec<u8>,
    owner: SymbolId,
) -> Result<(Unpickler<'a>, NodeId), Refusal> {
    let file = u.load_quote(bytes).map_err(|_| "a quote of unreadable TASTy")?;
    let root = *u.file(file).0.roots().first().ok_or("an empty quote")?;
    super::define_repeated(u)?;
    let p = Unpickler { u, tpd, names, file, meth: owner, owner, holes: Some(Vec::new()) };
    Ok((p, root))
}

impl Unpickler<'_> {
    /// The tree at `id` without the type definitions a quote with type holes starts with, and
    /// those definitions' symbols, each an alias of its type in `types` (dotty's `spliceTypes`).
    fn spliced_types(
        &mut self,
        id: NodeId,
        types: &[TypeId],
    ) -> Result<(NodeId, Vec<(SymbolId, TypeId)>), Refusal> {
        let k = self.kids(id);
        let spliced_def = |p: &Self, s: NodeId| {
            p.tag(s) == TYPEDEF && p.kids(s).first().is_some_and(|&r| p.tag(p.ast().deref(r)) == HOLE)
        };
        if self.tag(id) != BLOCK || k.len() < 2 || !k[1..].iter().all(|&s| spliced_def(self, s)) {
            return Ok((id, Vec::new()));
        }
        let mut spliced = Vec::new();
        for &def in &k[1..] {
            let hole = self.ast().deref(self.kids(def)[0]);
            let index = self.ast().node(hole).a as usize;
            let tpe = *types.get(index).ok_or("a quote type hole without its type")?;
            let sym = self.u.define(Symbol {
                name: self.name(def),
                kind: Kind::Type,
                owner: Some(self.owner),
                flags: Flags::default().with(tasty::tags::SYNTHETIC),
                target: None,
                is_extern: false,
                derived: false,
                decls: Vec::new(),
                tree: Some((self.file, def)),
            });
            self.u.unenter(sym);
            let alias = self.u.intern_type(Type::Alias(tpe));
            self.u.set_info(sym, alias);
            spliced.push((sym, tpe));
        }
        Ok((k[0], spliced))
    }

    /// Records the annotation classes of modifiers `mods` on `s`, a definition in a quote.
    pub fn annotate_quoted(&mut self, s: SymbolId, mods: &[NodeId]) {
        if self.holes.is_none() {
            return;
        }
        let annotations: Vec<NodeId> = mods.iter().copied().filter(|&m| self.tag(m) == ANNOTATION).collect();
        for m in annotations {
            let tpe = self.ty(self.kids(m)[0]);
            if let Some(c) = self.u.class_symbol(tpe) {
                self.u.annotate(s, c);
            }
        }
    }

    /// `HOLE`: a placeholder of the hole's type, recorded with its index and arguments.
    pub fn hole(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let k = self.kids(id);
        let index = self.ast().node(id).a as u32;
        let tpe = self.ty(k[0]);
        let mut args = Vec::new();
        for &a in &k[1..] {
            let arg = self.term(a)?;
            if matches!(self.tpd.tree(arg), T::TypeTree { .. }) {
                return Err("a quote hole with a type argument");
            }
            args.push(arg);
        }
        let name = TName { name: self.names.intern("<hole>"), is_type: false };
        let node = self.bare(T::Ident { name }, Some(tpe), None);
        self.holes.as_mut().ok_or("a hole outside a quote")?.push(QuoteHole { node, index, args });
        Ok(node)
    }
}
