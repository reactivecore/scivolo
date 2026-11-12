// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeUtils.scala
//! Class files from the lowered trees: a port of dotty's `GenBCode`.

mod access;
mod arrays;
mod body;
mod callee;
mod class;
mod class_bytes;
mod clinit;
mod closures;
mod concat;
mod cond;
mod control;
mod descriptors;
mod deserialize;
mod equality;
mod generic_infos;
mod java_annotations;
mod jdk_statics;
mod literals;
mod locals;
mod lub;
mod members;
mod method;
mod mirror;
mod monitors;
mod nested;
mod prims;
mod serial;
mod signatures;
mod stash;
mod switches;
mod traits;
mod tries;

pub use class_bytes::ClassBytes;

use lower::{Lower, Result, Unsupported};
use names::{name_kinds, nme};
use std::collections::HashMap;
use syntax::ast::Span;
use tasty::tags::{ABSTRACT, ARTIFACT, ENUM, FIELDaccessor, FINAL, MUTABLE, PRIVATE, STATIC, TRAIT};
use tasty::{SymbolId, Universe};
use typer::pickle::Pickled;
use typer::tpd::{TNodeId, Tree};

pub struct Gen<'l, 'a> {
    l: &'l mut Lower<'a>,
    out: Vec<ClassBytes>,
    /// The targets of the serializable lambdas of the class being generated.
    lambdas: Vec<classfile::Handle>,
    /// Classes by internal name: the unit's, and those `InnerClasses` looked up.
    classes: HashMap<String, Option<SymbolId>>,
    /// The TASTy of each top-level class not generated yet.
    tasty: HashMap<SymbolId, Vec<u8>>,
    /// The JDK's classes where the universe has no JDK.
    jdk: classfile::Jdk,
}

/// The class files of lowered unit `root`, `pickles` the TASTy of its top-level classes.
pub fn unit(l: &mut Lower, root: TNodeId, pickles: Vec<Pickled>) -> Result<Vec<ClassBytes>> {
    class_bytes::check_super_accessors(l, &pickles)?;
    let tasty = pickles.into_iter().map(|p| (p.class, p.tasty)).collect();
    let (out, lambdas, classes, jdk) = (Vec::new(), Vec::new(), HashMap::new(), classfile::Jdk::default());
    let mut g = Gen { l, out, lambdas, classes, tasty, jdk };
    g.collect_classes(root);
    let mut todo = vec![root];
    while let Some(t) = todo.pop() {
        match *g.l.tpd.tree(t) {
            Tree::PackageDef { stats, .. } => todo.extend(g.l.tpd.items(stats).iter().rev()),
            Tree::TypeDef { rhs, .. } if matches!(g.l.tpd.tree(rhs), Tree::Template { .. }) => {
                g.gen_class(t)?
            }
            _ => {}
        }
    }
    if !g.tasty.is_empty() {
        return unsupported(Span::NONE, "the TASTy of a class without a class file");
    }
    Ok(g.out)
}

fn unsupported<T>(at: Span, what: impl Into<String>) -> Result<T> {
    Err(Unsupported { at, what: what.into() })
}

/// The name of method `m` in class files: its `@targetName`, else its name.
fn method_name(u: &Universe, m: SymbolId) -> String {
    let sym = u.symbol(m);
    match u.is_constructor(m) {
        true if sym.owner.is_some_and(|o| u.is_trait(o)) => nme::TRAIT_CONSTRUCTOR.into(),
        true => nme::CONSTRUCTOR.into(),
        false => tasty::encode(sym.target.as_deref().unwrap_or(&sym.name)),
    }
}

/// Whether `s` is a static member: one of a static object's fields but a lazy value's field or
/// flag, or a Java static member.
fn is_static_member(l: &Lower, s: SymbolId) -> bool {
    let sym = l.u.symbol(s);
    let field = sym.kind == tasty::Kind::Val && !sym.flags.has(FIELDaccessor) && !l.u.is_module(s);
    let field = field && !l.is_lazy_storage(s);
    sym.flags.has(STATIC) || (field && sym.owner.is_some_and(|o| l.u.is_static_module(o)))
}

/// The access flags of member or class `s` (dotty's `javaFlags`).
fn java_flags(l: &Lower, s: SymbolId) -> u16 {
    use classfile::*;
    let u: &Universe = l.u;
    let (class, constructor) = (u.is_class(s), u.is_constructor(s));
    let owner = u.symbol(s).owner;
    let has = |tag| u.symbol(s).flags.has(tag);
    let top_module_constructor = constructor
        && owner.is_some_and(|o| {
            u.is_module_class(o)
                && u.symbol(o).owner.is_some_and(|p| u.symbol(p).kind == tasty::Kind::Package)
        });
    let private = !class && (has(PRIVATE) || top_module_constructor);
    let abstract_ = (class && has(ABSTRACT)) || has(TRAIT);
    let in_trait = owner.is_some_and(|o| u.is_trait(o));
    let method = u.symbol(s).kind == tasty::Kind::Def || has(FIELDaccessor);
    let final_ = (has(FINAL) || u.is_module_class(s) || u.is_module(s))
        && !constructor
        && !(has(MUTABLE) && !method)
        && !in_trait
        && !abstract_;
    let mut flags = if private { ACC_PRIVATE } else { ACC_PUBLIC };
    flags |= if final_ { ACC_FINAL } else { 0 };
    flags |= if abstract_ { ACC_ABSTRACT } else { 0 };
    flags |= if has(TRAIT) { ACC_INTERFACE } else { 0 };
    flags |= if is_static_member(l, s) && !class { ACC_STATIC } else { 0 };
    let outer = !class && name_kinds::is_outer(&u.symbol(s).name);
    flags |= if has(ARTIFACT) || outer { ACC_SYNTHETIC } else { 0 };
    flags |= if has(ENUM) { ACC_ENUM } else { 0 };
    flags | if class && !has(TRAIT) { ACC_SUPER } else { 0 }
}
