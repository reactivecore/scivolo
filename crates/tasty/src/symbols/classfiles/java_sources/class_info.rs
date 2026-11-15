//! The class file of a Java source's class as javac writes its signatures: access flags,
//! parents, generic signatures and descriptors of its members, inner-class entries and
//! annotations; what the class-file reader enters.

use super::resolve::{Resolved, Scope, resolve};
use super::syntax::{Arg, Class, ClassKind, Method, Type, TypeParam};
use super::{JavaSources, Located};
use crate::symbols::Universe;
use classfile::{ACC_STATIC, Annotation, ClassInfo, InnerClass, MemberInfo};
use names::nme;

/// The internal name of `java.lang.Object`.
pub const OBJECT: &str = "java/lang/Object";

/// The internal name of `java.lang.Enum`, an enum's superclass.
const ENUM: &str = "java/lang/Enum";

/// The internal name of `java.lang.annotation.Annotation`, an annotation type's parent.
const ANNOTATION: &str = "java/lang/annotation/Annotation";

/// The signatures of one class: its scope, the erasure of each type variable in scope.
pub struct Signatures<'a> {
    pub sources: &'a JavaSources,
    pub scope: Scope<'a>,
    pub erased_vars: Vec<(String, String)>,
}

/// Builds the class file of class `at`; the refusal of a name not found or a construct the
/// reader does not enter.
pub fn class_info(u: &mut Universe, sources: &JavaSources, at: &Located) -> Result<ClassInfo, String> {
    let class = at.class();
    let mut s = Signatures {
        sources,
        scope: Scope { at, vars: Vec::new(), in_header: true },
        erased_vars: Vec::new(),
    };
    if class.access & ACC_STATIC == 0 && at.outer().is_some() {
        let outer = at.outer().expect("an outer class");
        if !outer.class().type_params.is_empty() {
            return Err(format!("inner class `{}` of a generic Java class", class.name));
        }
    }
    let mut signature = s.type_params(u, &class.type_params)?;
    let superclass = match (class.kind, &class.extends) {
        (ClassKind::Enum, _) => {
            let this = Type::Named(vec![(class.name.clone(), Vec::new())]);
            qualified(ENUM, vec![Arg::Type(this)])
        }
        (ClassKind::Class, Some(t)) => t.clone(),
        _ => qualified(OBJECT, Vec::new()),
    };
    let mut interfaces = class.implements.clone();
    if class.kind == ClassKind::Annotation {
        interfaces.push(qualified(ANNOTATION, Vec::new()));
    }
    let (superclass_sig, superclass) = s.ty(u, &superclass)?;
    signature.push_str(&superclass_sig);
    let mut interface_names = Vec::new();
    for t in &interfaces {
        let (sig, desc) = s.ty(u, t)?;
        signature.push_str(&sig);
        interface_names.push(desc[1..desc.len() - 1].to_owned());
    }
    s.scope.in_header = false;
    let annotations = class.annotations.iter().map(|a| s.annotation(u, a)).collect::<Result<_, _>>()?;
    let fields = class.fields.iter().map(|f| {
        let (sig, descriptor) = s.ty(u, &f.ty)?;
        let (access, name, constant) = (f.access, f.name.clone(), f.constant.clone());
        Ok(MemberInfo { access, name, descriptor, signature: Some(sig), constant, annotation_default: false })
    });
    let fields = fields.collect::<Result<_, String>>()?;
    let methods = class.methods.iter().map(|m| s.method(u, at, m)).collect::<Result<_, String>>()?;
    Ok(ClassInfo {
        access: class.access,
        name: at.internal.clone(),
        superclass: Some(superclass[1..superclass.len() - 1].to_owned()),
        interfaces: interface_names,
        fields,
        methods,
        signature: Some(signature),
        annotations,
        inner_classes: inner_classes(at),
        scala: false,
    })
}

/// The type of the class of internal name `internal` with type arguments `args`.
fn qualified(internal: &str, args: Vec<Arg>) -> Type {
    let mut parts: Vec<(String, Vec<Arg>)> =
        internal.split('/').map(|n| (n.to_owned(), Vec::new())).collect();
    parts.last_mut().expect("a name").1 = args;
    Type::Named(parts)
}

/// The inner-class entries of class `at`: its own if it is nested, then its nested classes'.
fn inner_classes(at: &Located) -> Vec<InnerClass> {
    let entry = |c: &Class, outer: &str| InnerClass {
        inner: format!("{outer}${}", c.name),
        outer: Some(outer.to_owned()),
        name: Some(c.name.clone()),
        flags: c.access,
    };
    let own = at.outer().map(|o| entry(at.class(), &o.internal));
    own.into_iter().chain(at.class().nested.iter().map(|n| entry(n, &at.internal))).collect()
}

impl Signatures<'_> {
    /// The generic signature of type parameters `params` (`<T:…>`, empty without any), their
    /// names put in scope with their erasures, that of the first bound, which may name a later
    /// parameter (`<T extends U, U extends Number>`).
    fn type_params(&mut self, u: &mut Universe, params: &[TypeParam]) -> Result<String, String> {
        if params.is_empty() {
            return Ok(String::new());
        }
        self.scope.vars.extend(params.iter().map(|p| p.name.clone()));
        let first = self.erased_vars.len();
        self.erased_vars.extend(params.iter().map(|p| (p.name.clone(), format!("L{OBJECT};"))));
        for _ in params {
            for (i, p) in params.iter().enumerate() {
                if let Some(b) = p.bounds.first() {
                    self.erased_vars[first + i].1 = self.ty(u, b)?.1;
                }
            }
        }
        let mut out = String::from("<");
        for p in params {
            out.push_str(&p.name);
            if p.bounds.is_empty() {
                out.push_str(&format!(":L{OBJECT};"));
            }
            for b in &p.bounds {
                out.push(':');
                out.push_str(&self.ty(u, b)?.0);
            }
        }
        out.push('>');
        Ok(out)
    }

    /// The signature and descriptor of method `m` of class `at`; an inner class's constructor's
    /// descriptor takes the outer instance first, as javac writes it.
    fn method(&mut self, u: &mut Universe, at: &Located, m: &Method) -> Result<MemberInfo, String> {
        let outer_vars = (self.scope.vars.len(), self.erased_vars.len());
        let mut signature = self.type_params(u, &m.type_params)?;
        let mut descriptor = String::from("(");
        if m.name == nme::CONSTRUCTOR
            && at.class().access & ACC_STATIC == 0
            && let Some(outer) = at.outer()
        {
            descriptor.push_str(&format!("L{};", outer.internal));
        }
        signature.push('(');
        for p in &m.params {
            let (sig, desc) = self.ty(u, p)?;
            signature.push_str(&sig);
            descriptor.push_str(&desc);
        }
        let (sig, desc) = self.ty(u, &m.result)?;
        signature.push_str(&format!("){sig}"));
        descriptor.push_str(&format!("){desc}"));
        self.scope.vars.truncate(outer_vars.0);
        self.erased_vars.truncate(outer_vars.1);
        let (access, name, annotation_default) = (m.access, m.name.clone(), m.annotation_default);
        Ok(MemberInfo {
            access,
            name,
            descriptor,
            signature: Some(signature),
            constant: None,
            annotation_default,
        })
    }

    fn annotation(&mut self, u: &mut Universe, a: &super::syntax::Annotation) -> Result<Annotation, String> {
        let names: Vec<&str> = a.name.iter().map(String::as_str).collect();
        let Resolved::Class(c) = resolve(u, self.sources, &self.scope, &names)? else {
            return Err(format!("annotation `@{}` of a type variable", names.join(".")));
        };
        Ok(Annotation { descriptor: format!("L{c};"), enum_elements: a.enum_elements.clone() })
    }
}
