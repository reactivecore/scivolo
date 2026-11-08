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
mod deserialize;
mod equality;
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
mod signatures;
mod switches;
mod traits;
mod tries;

pub use class_bytes::ClassBytes;

use lower::{Lower, Result, Unsupported, jdk_class_name};
use std::collections::HashMap;
use syntax::ast::Span;
use tasty::tags::{ABSTRACT, ARTIFACT, ENUM, FIELDaccessor, FINAL, MUTABLE, PRIVATE, STATIC, TRAIT};
use tasty::{Erased, SymbolId, Universe};
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

/// The internal name of the class of binary name `binary`, as the JDK names it.
fn internal(binary: &str) -> String {
    match binary {
        "scala.Any" | "scala.AnyRef" => "java/lang/Object".into(),
        _ => jdk_class_name(binary).replace('.', "/"),
    }
}

/// The field descriptor of erased type `e`.
fn descriptor(e: &Erased) -> String {
    match e {
        Erased::Prim(c) => match c {
            'z' => "Z",
            'c' => "C",
            'b' => "B",
            's' => "S",
            'i' => "I",
            'j' => "J",
            'f' => "F",
            _ => "D",
        }
        .into(),
        Erased::Unit => "V".into(),
        Erased::Nothing => "Lscala/runtime/Nothing$;".into(),
        Erased::Null => "Lscala/runtime/Null$;".into(),
        Erased::Class(name) | Erased::Value(name, _) => format!("L{};", internal(name)),
        Erased::Array(elem) => format!("[{}", descriptor(elem)),
    }
}

/// The internal name of a class or array type of erased type `e`, as `ldc` and `new` take it.
fn class_internal(e: &Erased) -> String {
    match e {
        Erased::Class(name) | Erased::Value(name, _) => internal(name),
        Erased::Nothing => "scala/runtime/Nothing$".into(),
        Erased::Null => "scala/runtime/Null$".into(),
        e => descriptor(e),
    }
}

/// The class whose `TYPE` field holds the class of primitive erased type `e`.
fn primitive_box(e: &Erased) -> Option<&'static str> {
    let Erased::Prim(c) = e else { return (*e == Erased::Unit).then_some("java/lang/Void") };
    Some(match c {
        'z' => "java/lang/Boolean",
        'c' => "java/lang/Character",
        'b' => "java/lang/Byte",
        's' => "java/lang/Short",
        'i' => "java/lang/Integer",
        'j' => "java/lang/Long",
        'f' => "java/lang/Float",
        _ => "java/lang/Double",
    })
}

/// The method descriptor of method `m`; `V` as the result of a constructor.
fn method_descriptor(u: &mut Universe, m: SymbolId) -> String {
    let (params, result) = u.erased_method(m);
    let result = if u.is_constructor(m) { Erased::Unit } else { result };
    let params: String = params.iter().map(descriptor).collect();
    format!("({params}){}", descriptor(&result))
}

/// The name of method `m` in class files: its `@targetName`, else its name.
fn method_name(u: &Universe, m: SymbolId) -> String {
    let sym = u.symbol(m);
    match u.is_constructor(m) {
        true if sym.owner.is_some_and(|o| u.is_trait(o)) => "$init$".into(),
        true => "<init>".into(),
        false => tasty::encode(sym.target.as_deref().unwrap_or(&sym.name)),
    }
}

/// Whether `s` is an object's class reached through static paths only.
fn is_static_module(u: &Universe, s: SymbolId) -> bool {
    u.is_module_class(s) && u.is_static_symbol(s)
}

/// Whether `s` is a static member: one of a static object's fields but a lazy value's field or
/// flag, or a Java static member.
fn is_static_member(l: &Lower, s: SymbolId) -> bool {
    let sym = l.u.symbol(s);
    let field = sym.kind == tasty::Kind::Val && !sym.flags.has(FIELDaccessor) && !l.u.is_module(s);
    let field = field && !l.is_lazy_storage(s);
    sym.flags.has(STATIC) || (field && sym.owner.is_some_and(|o| is_static_module(l.u, o)))
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
    let final_ = (has(FINAL) || u.is_module_class(s) || u.is_module(s))
        && !constructor
        && !has(MUTABLE)
        && !in_trait
        && !abstract_;
    let mut flags = if private { ACC_PRIVATE } else { ACC_PUBLIC };
    flags |= if final_ { ACC_FINAL } else { 0 };
    flags |= if abstract_ { ACC_ABSTRACT } else { 0 };
    flags |= if has(TRAIT) { ACC_INTERFACE } else { 0 };
    flags |= if is_static_member(l, s) && !class { ACC_STATIC } else { 0 };
    let outer = !class && (u.symbol(s).name == "$outer" || u.symbol(s).name.ends_with("$$$outer"));
    flags |= if has(ARTIFACT) || outer { ACC_SYNTHETIC } else { 0 };
    flags |= if has(ENUM) { ACC_ENUM } else { 0 };
    flags | if class && !has(TRAIT) { ACC_SUPER } else { 0 }
}
