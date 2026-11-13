// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Pickler.scala
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//   compiler/src/dotty/tools/dotc/core/tasty/AttributePickler.scala
//! The TASTy of each top-level class of a source file (dotty's `Pickler.run`, `sliceTopLevel`)
//! and the sections only a file's TASTy has.

use super::names::sym_name;
use super::{QuoteSource, Refusal, TreePickler, pickle};
use crate::defn::type_ref;
use crate::tpd::{List, TNodeId, Tpd, Tree as T};
use rustc_hash::FxHashMap;
use syntax::names::Names;
use tasty::tags::{IDENTtpt, INLINED};
use tasty::{Kind, SymbolId, Universe};

const SCALA2STANDARDLIBRARY: u8 = 1;
const EXPLICITNULLS: u8 = 2;
const SOURCEFILE: u8 = 129;

/// The attributes of a file's TASTy (dotty's `Attributes`).
#[derive(Clone, Copy)]
pub struct FileAttributes {
    pub scala2: bool,
    pub explicit_nulls: bool,
}

/// A top-level class, the TASTy of it and its companion, and the super accessors it declares.
pub struct Pickled {
    pub class: SymbolId,
    pub tasty: Vec<u8>,
    pub super_accessors: super::Accessors,
}

/// What pickling a file's top-level class knows besides the trees.
pub struct File<'a> {
    pub attributes: FileAttributes,
    /// The annotation trees of each definition, in order.
    pub annotations: &'a FxHashMap<SymbolId, Vec<TNodeId>>,
    /// The top-level class pickled (dotty's `rootCls`).
    pub root_class: SymbolId,
}

/// The TASTy of the top-level classes of package clause `root` of `source`, but of the
/// object classes whose companion class is among them; `annotations` holds the annotation
/// trees of each definition, in order.
pub fn pickle_file(
    (u, tpd, names): (&mut Universe, &mut Tpd, &Names),
    root: TNodeId,
    source: &QuoteSource,
    (attributes, annotations): (FileAttributes, &FxHashMap<SymbolId, Vec<TNodeId>>),
) -> Result<Vec<Pickled>, Refusal> {
    if !matches!(tpd.tree(root), T::PackageDef { .. }) {
        return Err("a source file that is not one package clause");
    }
    let mut classes = Vec::new();
    top_level_classes(u, tpd, root, &mut classes);
    let companions: Vec<SymbolId> =
        classes.iter().filter_map(|&c| (!u.is_module_class(c)).then(|| linked_class(u, c))?).collect();
    classes.retain(|c| !companions.contains(c));
    let mut out = Vec::new();
    for class in classes {
        let Some(sliced) = slice(u, tpd, root, class) else { continue };
        let file = File { attributes, annotations, root_class: class };
        let (tasty, super_accessors) = pickle((u, &*tpd, names), sliced, &[], Some(source), Some(&file))?;
        let parsed = tasty::Tasty::parse(&tasty).ok();
        let trees = parsed.as_ref().and_then(|t| tasty::Ast::parse(t.section("ASTs")?.bytes).ok());
        trees.ok_or("trees whose TASTy does not read back")?;
        out.push(Pickled { class, tasty, super_accessors });
    }
    Ok(out)
}

fn top_level_classes(u: &Universe, tpd: &Tpd, t: TNodeId, out: &mut Vec<SymbolId>) {
    match tpd.tree(t) {
        &T::PackageDef { stats, .. } => {
            tpd.items(stats).iter().for_each(|&s| top_level_classes(u, tpd, s, out))
        }
        T::TypeDef { .. } => out.extend(tpd.sym(t).filter(|&s| u.is_class(s))),
        _ => {}
    }
}

/// The module class of class `c`'s companion, the companion class of module class `c`
/// (dotty's `linkedClass`).
fn linked_class(u: &mut Universe, c: SymbolId) -> Option<SymbolId> {
    match u.is_module_class(c) {
        true => u.companion_class(c),
        false => u.companion_module(c).and_then(|m| u.module_class(m)),
    }
}

/// Tree `t` with the top-level definitions of `class` and its companion only, none if it has
/// nothing but imports (dotty's `sliceTopLevel`).
fn slice(u: &mut Universe, tpd: &mut Tpd, t: TNodeId, class: SymbolId) -> Option<TNodeId> {
    let sym = tpd.sym(t);
    match *tpd.tree(t) {
        T::PackageDef { pid, stats } => {
            let items = tpd.items(stats).to_vec();
            let kept: Vec<TNodeId> = items.into_iter().filter_map(|s| slice(u, tpd, s, class)).collect();
            if kept.iter().all(|&s| matches!(tpd.tree(s), T::Import { .. })) {
                return None;
            }
            let stats = tpd.list(&kept);
            let node = tpd.get(t);
            let (span, tpe) = (node.span, node.tpe);
            Some(tpd.node(T::PackageDef { pid, stats }, span, tpe, sym))
        }
        T::TypeDef { .. } => {
            let s = sym?;
            (s == class || linked_class(u, s) == Some(class)).then_some(t)
        }
        T::ValDef { .. } => {
            let s = sym?;
            let module_class = u.module_class(s);
            let companion = module_class.and_then(|m| u.companion_class(m));
            (companion == Some(class) || module_class == Some(class)).then_some(t)
        }
        _ => Some(t),
    }
}

/// The class that is or, for an object, whose class is the top-level definition `s` is in
/// (dotty's `topLevelClass`).
fn top_level_class(u: &mut Universe, mut s: SymbolId) -> Option<SymbolId> {
    while let Some(owner) = u.symbol(s).owner.filter(|&o| u.symbol(o).kind != Kind::Package) {
        s = owner;
    }
    match u.is_class(s) {
        true => Some(s),
        false => u.module_class(s),
    }
}

impl TreePickler<'_> {
    /// Whether `s` is defined in the top-level class pickled or its companion (dotty's
    /// `isLocallyDefined`).
    pub fn is_locally_defined(&mut self, s: SymbolId) -> bool {
        let Some(root) = self.file.map(|f| f.root_class) else { return false };
        let top = top_level_class(self.u, s);
        top == Some(root) || top.is_some_and(|t| linked_class(self.u, t) == Some(root))
    }

    /// Inline expansion `expansion` of `call` after `bindings`, in a file the call as a reference
    /// to the top-level class of the method inlined (dotty's `inlineCallTrace` in PostTyper).
    pub fn inlined(
        &mut self,
        call: Option<TNodeId>,
        bindings: List,
        expansion: TNodeId,
    ) -> Result<(), Refusal> {
        let Some(call) = call.filter(|_| self.file.is_some()) else {
            self.pre_register(self.tpd.items(bindings));
            let call: Vec<TNodeId> = call.into_iter().collect();
            return self.node(INLINED, &[&[expansion][..], &call].concat(), &[bindings]);
        };
        let mut fun = call;
        while let T::Apply { fun: f, .. } | T::TypeApply { fun: f, .. } = *self.tpd.tree(fun) {
            fun = f;
        }
        let method = self.tpd.sym(fun).ok_or("an inline call of no method")?;
        let class = top_level_class(self.u, method).ok_or("an inline method outside a class")?;
        let bindings = self.tpd.items(bindings).to_vec();
        self.pre_register(&bindings);
        self.call_traces.insert(call);
        self.with_length(INLINED, &mut |p| {
            p.tree(expansion)?;
            p.buf.register_tree(p.tpd.copy_of(call).unwrap_or(call).index() as u32);
            p.byte(IDENTtpt);
            p.name(&sym_name(p.u, class));
            let tpe = type_ref(p.u, class);
            p.ty(tpe, false)?;
            bindings.iter().try_for_each(|&b| p.tree(b))
        })
    }

    /// The Comments section, empty, and the Attributes section of a file, its source the one of
    /// the Positions section (dotty's `pickleComments`, `pickleAttributes`).
    pub fn file_sections(&mut self, attributes: &FileAttributes, source: &str) {
        let comments = self.pickler.new_section("Comments");
        self.pickler.set_section(comments, Vec::new());
        let section = self.pickler.new_section("Attributes");
        let mut buf = tasty::write::Buffer::default();
        let flags = [(attributes.scala2, SCALA2STANDARDLIBRARY), (attributes.explicit_nulls, EXPLICITNULLS)];
        flags.iter().filter(|(on, _)| *on).for_each(|&(_, tag)| buf.write_byte(tag));
        let source = self.pickler.names.utf8(source);
        buf.write_byte(SOURCEFILE);
        buf.write_nat(source.0 as u64);
        self.pickler.set_section(section, buf.bytes);
    }
}
