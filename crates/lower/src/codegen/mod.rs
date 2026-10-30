// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirCodeGen.scala
//! NIR generation from the lowered trees: a port of Scala Native's nscplugin.

pub(crate) mod annotations;
mod apply;
mod args;
mod array;
mod attrs;
mod body;
mod branch;
mod buf;
mod c_types;
mod callbacks;
pub(crate) mod callee;
mod cast;
mod class;
mod closure;
mod closure_methods;
mod concat;
mod conversions;
mod cquote;
mod dead;
mod dynmethod;
mod equality;
mod erased;
mod exports;
mod expr;
mod extern_calls;
mod extern_fields;
mod extern_members;
mod extern_varargs;
mod externs;
mod fields;
mod finally;
pub(crate) mod funcptr;
mod interface_bridges;
pub(crate) mod intrinsics;
mod labels;
mod layout;
mod lazy_handles;
mod linktime;
mod linktime_defs;
mod literal;
mod method;
mod modules;
mod names;
mod numeric;
mod ops;
mod positions;
mod primitives;
mod statics;
mod structs;
mod synchronized;
mod tries;
mod unit_defs;

use crate::{Lower, NirFile, Result};
use method::Method;
use nir::encode::Writer;
use nir::{Attr, Defn, DefnKind, Global, GlobalId, Pool, Pos, PosId, StrId, Type, TypeId, Val, ValId};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::TNodeId;
use unit_defs::{definition_spans, type_defs};

pub struct Gen<'l, 'a> {
    pub l: &'l mut Lower<'a>,
    pub pool: Pool,
    defns: Vec<Defn>,
    /// Mirror classes, each with its static forwarders.
    mirrors: Vec<Vec<Defn>>,
    class: Option<SymbolId>,
    m: Method,
    /// The position of what has none.
    no_pos: PosId,
    /// The spans of the unit's classes and methods.
    spans: FxHashMap<SymbolId, Span>,
    /// The unit's abstract methods.
    deferred: FxHashSet<SymbolId>,
    /// The anonymous classes of the class's closures.
    closures: Vec<Defn>,
    /// The number of the class's closures so far.
    lambdas: u32,
    /// The C struct type of each class by binary name, none for a class that is not `@struct`.
    structs: FxHashMap<String, Option<TypeId>>,
}

/// The NIR files of unit `root`, one per top-level name, in the order first defined.
pub fn unit(l: &mut Lower, root: TNodeId) -> Result<Vec<NirFile>> {
    let mut pool = Pool::default();
    // Interns the file names first, ahead of every other string of the file.
    for (_, source) in l.sources.files() {
        pool.strings.intern_str(&source.name);
    }
    let virtual_file = StrId(pool.strings.intern_str(""));
    let no_pos = PosId(pool.positions.intern(Pos { file: virtual_file, line: 0, column: 0 }));
    let m = Method::new(None, 0, no_pos);
    let type_defs = type_defs(l, root);
    let mut spans = definition_spans(l, &type_defs);
    spans.extend(l.lazy_fields.iter().map(|f| (f.accessor, f.span)));
    let deferred = interface_bridges::deferred_methods(l.tpd, &type_defs);
    let mut g = Gen {
        l,
        pool,
        defns: Vec::new(),
        mirrors: Vec::new(),
        class: None,
        m,
        no_pos,
        spans,
        deferred,
        closures: Vec::new(),
        lambdas: 0,
        structs: FxHashMap::default(),
    };
    g.gen_companion_handle_fields(&type_defs)?;
    for td in type_defs {
        g.gen_class(td)?;
    }
    let classes: Vec<String> = g
        .defns
        .iter()
        .filter(|d| matches!(d.kind, DefnKind::Class { .. }))
        .map(|d| g.pool.mangle(d.name).to_lowercase())
        .collect();
    g.mirrors.retain(|ds| !classes.contains(&g.pool.mangle(ds[0].name).to_lowercase()));
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
}
