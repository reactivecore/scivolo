// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/classfile/ClassfileParser.scala
//! A class of a class file entered (dotty's `parseClass`, `parseMember`): the class with its
//! type parameters, parents and instance members, its object's class with the static ones.

use super::super::builtin::synthetic;
use super::super::{Kind, SymbolId, Universe};
use super::annotation;
use super::constant::constant_type;
use super::signatures::{Sig, Var, bounded_by_object, bounds, cooked};
use crate::tags::{ABSTRACT, FINAL, MUTABLE, OBJECT, PROTECTED, SEALED, STATIC, TRAIT};
use crate::types::{NameId, Type, TypeId};
use classfile::{
    ACC_ABSTRACT, ACC_BRIDGE, ACC_ENUM, ACC_FINAL, ACC_INTERFACE, ACC_NATIVE, ACC_PROTECTED, ACC_PUBLIC,
    ACC_STATIC, ACC_SYNTHETIC, ACC_VARARGS, ClassInfo, MemberInfo,
};
use names::nme;

/// The class being entered: it, its object's class, and its type parameters with their bounds'
/// text.
pub struct Owners<'a> {
    pub class: SymbolId,
    pub statics: SymbolId,
    pub params: Vec<(String, &'a str)>,
    pub vars: Vec<Var>,
}

/// Enters class `info` into package or class `pkg` as `simple` with its object: the class, the
/// object's class and the object; none, with nothing entered, if its signature is one this
/// reader refuses.
pub fn enter(u: &mut Universe, pkg: SymbolId, simple: &str, info: &ClassInfo) -> Option<Vec<SymbolId>> {
    let interface = info.access & ACC_INTERFACE != 0;
    let mut flags = Vec::new();
    let sealed = info.access & ACC_ENUM != 0 && info.fields.iter().any(|f| f.access & ACC_ENUM != 0);
    flags.extend(if interface { &[TRAIT, ABSTRACT][..] } else { &[] });
    flags.extend((!interface && (sealed || info.access & ACC_ABSTRACT != 0)).then_some(ABSTRACT));
    flags.extend((info.access & ACC_FINAL != 0).then_some(FINAL));
    flags.extend(sealed.then_some(SEALED));
    let class = synthetic(u, pkg, simple, Kind::Class, &flags);
    let object = u.types.intern(Type::Sym(None, u.defn().object));
    let statics = synthetic(u, pkg, &format!("{simple}$"), Kind::Class, &[OBJECT, FINAL]);
    u.parents.insert(statics, vec![object]);
    let module = synthetic(u, pkg, simple, Kind::Val, &[OBJECT, FINAL]);
    let module_type = u.types.intern(Type::Sym(None, statics));
    u.infos.insert(module, module_type);
    let entered = [class, statics, module];
    let Ok(owners) = class_signature(u, class, statics, info) else {
        entered.into_iter().for_each(|s| u.unenter(s));
        return None;
    };
    u.java_stubs.extend(entered);
    u.classfile_classes.extend(entered);
    let members = info.fields.iter().map(|f| (f, false)).chain(info.methods.iter().map(|m| (m, true)));
    let members: Vec<SymbolId> =
        members.filter_map(|(m, method)| member(u, &owners, m, method).ok()?).collect();
    annotation::enter(u, class, info, &members);
    Some(entered.to_vec())
}

/// The class's type parameters entered and its parents set, from its signature or else its
/// superclass and interfaces; `Object` for `<FromJavaObject>` among the parents.
fn class_signature<'a>(
    u: &mut Universe,
    class: SymbolId,
    statics: SymbolId,
    info: &'a ClassInfo,
) -> Result<Owners<'a>, String> {
    let object = u.types.intern(Type::Sym(None, u.defn().object));
    let from_java_object = u.types.intern(Type::Sym(None, u.defn().from_java_object));
    let Some(signature) = info.signature.as_deref() else {
        let names = info.superclass.iter().chain(&info.interfaces);
        let parents = names.map(|n| if n == "java/lang/Object" { object } else { cooked(u, n) }).collect();
        u.parents.insert(class, parents);
        return Ok(Owners { class, statics, params: Vec::new(), vars: Vec::new() });
    };
    let mut sig = Sig::new(signature);
    let params = sig.type_params(u)?;
    let mut vars = Vec::new();
    let mut syms = Vec::new();
    for (name, b) in &params {
        let p = synthetic(u, class, name, Kind::TypeParam, &[]);
        syms.push(p);
        vars.push((name.clone(), u.types.intern(Type::Sym(None, p)), bounded_by_object(b)));
    }
    for (&p, (_, b)) in syms.iter().zip(&params) {
        let bounds = bounds(u, b, &vars)?;
        u.infos.insert(p, bounds);
    }
    let mut parents = Vec::new();
    while !sig.at_end() {
        let parent = sig.ty(u, &vars)?;
        parents.push(if parent == from_java_object { object } else { parent });
    }
    u.parents.insert(class, parents);
    Ok(Owners { class, statics, params, vars })
}

/// Enters field or method `m` unless it is private, package-private, synthetic, a bridge, a
/// static initializer or signature polymorphic (native varargs, dotty's `isSignaturePolymorphic`),
/// or its signature is one this reader refuses: the member entered, or the refusal.
pub fn member(
    u: &mut Universe,
    owners: &Owners,
    m: &MemberInfo,
    method: bool,
) -> Result<Option<SymbolId>, String> {
    let visible = m.access & (ACC_PUBLIC | ACC_PROTECTED) != 0;
    let artifact = m.access & ACC_SYNTHETIC != 0 || method && m.access & ACC_BRIDGE != 0;
    let polymorphic = method && m.access & (ACC_NATIVE | ACC_VARARGS) == ACC_NATIVE | ACC_VARARGS;
    if !visible || artifact || m.name == nme::STATIC_CONSTRUCTOR {
        return Ok(None);
    }
    if polymorphic {
        return Err(format!("signature polymorphic method {}", m.name));
    }
    let is_static = m.access & ACC_STATIC != 0;
    let owner = if is_static { owners.statics } else { owners.class };
    let text = m.signature.as_deref().unwrap_or(&m.descriptor);
    let info = match (method, &m.constant) {
        (true, _) => method_type(u, owners, m, text),
        (false, Some(c)) => constant_type(u, &m.descriptor, c),
        (false, _) => Sig::new(text).ty(u, &owners.vars),
    };
    let info = info?;
    let mut flags = Vec::new();
    flags.extend((m.access & ACC_PROTECTED != 0).then_some(PROTECTED));
    if method {
        flags.extend((m.access & ACC_FINAL != 0).then_some(FINAL));
    } else {
        flags.extend(is_static.then_some(STATIC));
        flags.push(if m.access & ACC_FINAL != 0 { FINAL } else { MUTABLE });
    }
    let kind = if method { Kind::Def } else { Kind::Val };
    let s = synthetic(u, owner, &m.name, kind, &flags);
    u.infos.insert(s, info);
    u.classfile_descriptors.insert(s, m.descriptor.clone());
    if method {
        u.set_deferred(s, m.access & ACC_ABSTRACT != 0);
    }
    Ok(Some(s))
}

/// The type of method `m` of signature `text`: polymorphic in its type parameters, a
/// constructor in the class's instead, returning the class's type (dotty's
/// `normalizeConstructorInfo`); a constructor with type parameters of its own is refused.
fn method_type(u: &mut Universe, owners: &Owners, m: &MemberInfo, text: &str) -> Result<TypeId, String> {
    let mut sig = Sig::new(text);
    let own = sig.type_params(u)?;
    let constructor = m.name == nme::CONSTRUCTOR;
    if constructor && !own.is_empty() {
        return Err(format!("constructor of signature `{text}` with type parameters of its own"));
    }
    let mut params: Vec<(String, &str)> = if constructor { owners.params.clone() } else { Vec::new() };
    params.extend(own.iter().map(|(n, b)| (n.clone(), *b)));
    let binder = u.fresh_binder();
    let mut vars = if constructor { Vec::new() } else { owners.vars.clone() };
    for (i, (name, b)) in params.iter().enumerate() {
        let param = u.types.intern(Type::Param(binder, i as u32));
        vars.push((name.clone(), param, bounded_by_object(b)));
    }
    let bounds: Vec<(NameId, TypeId)> = params
        .iter()
        .map(|(n, b)| Ok((u.types.intern_name(n), bounds(u, b, &vars)?)))
        .collect::<Result<_, String>>()?;
    let (param_types, mut result) = sig.method(u, &vars, m.access & ACC_VARARGS != 0)?;
    if constructor {
        let class = u.types.intern(Type::Sym(None, owners.class));
        let args: Vec<TypeId> = vars[..owners.params.len()].iter().map(|&(_, t, _)| t).collect();
        result = if args.is_empty() { class } else { u.types.intern_applied(class, &args) };
    }
    let names: Vec<_> = param_types
        .into_iter()
        .enumerate()
        .map(|(i, t)| (u.types.intern_name(&format!("x${i}")), t))
        .collect();
    let mbinder = u.fresh_binder();
    let method = u.types.intern_method(mbinder, &names, result, false, false);
    Ok(match bounds.is_empty() {
        true => method,
        false => u.types.intern_poly(binder, &bounds, method),
    })
}
