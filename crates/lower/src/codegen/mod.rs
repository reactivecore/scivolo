//! NIR generation from the lowered trees: a port of Scala Native's nscplugin without native
//! interop.

mod apply;
mod body;
mod branch;
mod buf;
mod class;
mod dead;
mod expr;
mod method;
mod names;
mod ops;
mod statics;
mod tries;
mod types;

use crate::{Lower, NirFile, Result};
use method::Method;
use nir::encode::Writer;
use nir::{Attr, Defn, DefnKind, Global, GlobalId, Pool, Pos, PosId, StrId, Type, TypeId, Val, ValId};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

pub struct Gen<'l, 'a> {
    pub l: &'l mut Lower<'a>,
    pub pool: Pool,
    defns: Vec<Defn>,
    /// Mirror classes, each with its static forwarders.
    mirrors: Vec<Vec<Defn>>,
    class: Option<SymbolId>,
    m: Method,
    file: StrId,
}

/// The NIR files of unit `root`, one per top-level name, in the order first defined.
pub fn unit(l: &mut Lower, root: TNodeId) -> Result<Vec<NirFile>> {
    let mut pool = Pool::default();
    let file = StrId(pool.strings.intern_str(&l.source.name));
    let none = PosId(pool.positions.intern(Pos { file, line: 0, column: 0 }));
    let m = Method::new(None, 0, none);
    let mut g = Gen { l, pool, defns: Vec::new(), mirrors: Vec::new(), class: None, m, file };
    for td in type_defs(g.l, root) {
        g.gen_class(td)?;
    }
    let mut groups: Vec<(GlobalId, Vec<Defn>)> = Vec::new();
    for d in std::mem::take(&mut g.defns) {
        let top = g.top_of(d.name);
        match groups.iter_mut().find(|(t, _)| *t == top) {
            Some((_, ds)) => ds.push(d),
            None => groups.push((top, vec![d])),
        }
    }
    let mirrors = std::mem::take(&mut g.mirrors);
    groups.extend(mirrors.into_iter().map(|ds| (g.top_of(ds[0].name), ds)));
    let none = GlobalId(g.pool.globals.intern(Global::None));
    let files = groups
        .into_iter()
        .map(|(top, defns)| {
            let entry_points = defns.iter().any(|d| g.is_entry_point(d));
            let bytes = Writer::new(&g.pool).file(&defns, none, entry_points);
            NirFile { name: g.pool.top_name(top).to_owned(), bytes }
        })
        .collect();
    Ok(files)
}

/// The class definitions of the unit's packages.
fn type_defs(l: &Lower, root: TNodeId) -> Vec<TNodeId> {
    let mut out = Vec::new();
    let mut todo = vec![root];
    while let Some(t) = todo.pop() {
        match *l.tpd.tree(t) {
            Tree::PackageDef { stats, .. } => todo.extend(l.tpd.items(stats).iter().rev()),
            Tree::TypeDef { .. } => out.push(t),
            _ => {}
        }
    }
    out
}

impl Gen<'_, '_> {
    fn top_of(&mut self, g: GlobalId) -> GlobalId {
        match *self.pool.global(g) {
            Global::Member(owner, _) => owner,
            _ => g,
        }
    }

    /// Whether the linker starts from `d`: a static initializer or an extern definition.
    fn is_entry_point(&self, d: &Defn) -> bool {
        let clinit = self.pool.member_sig(d.name) == Some("IE");
        matches!(d.kind, DefnKind::Define { .. }) && (clinit || d.attrs.iter().any(Attr::is_extern))
    }

    pub fn ty(&mut self, t: Type) -> TypeId {
        TypeId(self.pool.types.intern(t))
    }

    pub fn val(&mut self, v: Val) -> ValId {
        ValId(self.pool.vals.intern(v))
    }

    pub fn str(&mut self, s: &str) -> StrId {
        StrId(self.pool.strings.intern_str(s))
    }

    /// The position of `span`'s point, or `None` if it has none.
    pub fn pos_of(&mut self, span: Span) -> Option<PosId> {
        if !span.exists() {
            return None;
        }
        let src = self.l.source;
        let line = src.offset_to_line(span.point()) as u32;
        let column = src.column(span.point()) as u32;
        Some(PosId(self.pool.positions.intern(Pos { file: self.file, line, column })))
    }

    /// The position of `span`, or `or` if it has none.
    pub fn pos_or(&mut self, span: Span, or: PosId) -> PosId {
        self.pos_of(span).unwrap_or(or)
    }
}
