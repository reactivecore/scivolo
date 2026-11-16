// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/JavaParsers.scala
//! The annotations of the fields, methods and constructors of the Java sources' classes, their
//! names resolved to symbols.

use super::super::classpath::java_class;
use super::super::signatures::cooked;
use super::resolve::{Resolved, Scope, resolve};
use super::syntax::{Annotation, ArgValue};
use super::{JavaSources, Located};
use crate::symbols::{Kind, SymbolId, Universe};
use classfile::{ACC_STATIC, ElementValue, MemberInfo};

/// An annotation of a member of a Java source: its class and its arguments by element.
pub struct SourceAnnotation {
    pub class: SymbolId,
    pub args: Vec<(String, SourceArg)>,
}

/// The value of an annotation element of a Java source.
pub enum SourceArg {
    Literal(ElementValue),
    Array(Vec<SourceArg>),
    /// A static field: an enum constant or a constant.
    Static(SymbolId),
    Annotation(SourceAnnotation),
}

/// The annotations of a member that dotty keeps, or the refusal of one this reader does not
/// resolve.
pub type MemberAnnotations = Result<Vec<SourceAnnotation>, String>;

impl Universe {
    /// Enters the classes of the Java sources and determines the annotations of their fields,
    /// methods and constructors, an annotation with an argument dotty's parser does not keep left
    /// out (dotty's `JavaParsers.annotation`). A member whose annotations the parser does not
    /// read, an enum constant's or a record's, is left out; a class literal or a name other than a
    /// static field of a class as argument is refused.
    pub fn java_source_annotations(&mut self) -> Vec<(SymbolId, MemberAnnotations)> {
        let Some(sources) = self.java_sources() else { return Vec::new() };
        let mut out = Vec::new();
        for at in sources.classes() {
            let Some(info) = java_class(self, &at.internal) else { continue };
            let Some(class) = class_symbol(self, &at.internal) else { continue };
            let statics = self.companion_module(class).and_then(|m| self.module_class(m));
            let c = at.class();
            let written =
                c.fields.iter().map(|f| &f.annotations).chain(c.methods.iter().map(|m| &m.annotations));
            for (annots, m) in written.zip(info.fields.iter().chain(&info.methods)) {
                let owner = if m.access & ACC_STATIC != 0 { statics } else { Some(class) };
                let (Some(annots), Some(sym)) = (annots, owner.and_then(|o| self.member_of(o, m))) else {
                    continue;
                };
                let scope = Scope { at: &at, vars: Vec::new(), in_header: false };
                let resolved = annots.iter().map(|a| annotation(self, &sources, &scope, a));
                let resolved: Result<Vec<_>, String> = resolved.filter_map(Result::transpose).collect();
                out.push((sym, resolved));
            }
        }
        out
    }

    /// The member of `owner` entered from member `m` of a class file.
    fn member_of(&self, owner: SymbolId, m: &MemberInfo) -> Option<SymbolId> {
        let decls = self.symbol(owner).decls.iter().copied();
        decls.into_iter().find(|&d| {
            self.symbol(d).name == m.name && self.classfile_descriptors.get(&d) == Some(&m.descriptor)
        })
    }
}

/// The class of internal name `internal`, entered.
fn class_symbol(u: &mut Universe, internal: &str) -> Option<SymbolId> {
    let t = cooked(u, internal);
    u.class_symbol(t)
}

/// Annotation `a` resolved in `scope`; none if dotty's parser drops it.
fn annotation(
    u: &mut Universe,
    sources: &JavaSources,
    scope: &Scope,
    a: &Annotation,
) -> Result<Option<SourceAnnotation>, String> {
    let Some(args) = &a.args else { return Ok(None) };
    let names: Vec<&str> = a.name.iter().map(String::as_str).collect();
    let Resolved::Class(internal) = resolve(u, sources, scope, &names)? else {
        return Err(format!("annotation `@{}` of a type variable", names.join(".")));
    };
    let class = class_symbol(u, &internal).ok_or_else(|| format!("annotation class `{internal}`"))?;
    let args = args.iter().map(|(n, v)| Ok((n.clone(), arg(u, sources, scope, v)?)));
    Ok(Some(SourceAnnotation { class, args: args.collect::<Result<_, String>>()? }))
}

/// Annotation argument `v` resolved in `scope`.
fn arg(u: &mut Universe, sources: &JavaSources, scope: &Scope, v: &ArgValue) -> Result<SourceArg, String> {
    Ok(match v {
        ArgValue::Literal(e) => SourceArg::Literal(e.clone()),
        ArgValue::Array(vs) => {
            SourceArg::Array(vs.iter().map(|v| arg(u, sources, scope, v)).collect::<Result<_, _>>()?)
        }
        ArgValue::Annotation(a) => match annotation(u, sources, scope, a)? {
            Some(a) => SourceArg::Annotation(a),
            None => return Err("a nested annotation dotty drops".to_owned()),
        },
        ArgValue::Path(path) => SourceArg::Static(static_field(u, sources, scope.at, path)?),
        ArgValue::ClassLiteral => return Err("a class literal as Java annotation argument".to_owned()),
    })
}

/// The static field name `path` stands for in class `at`: `C.f` a field of class `C`, `f` one of
/// `at` or a class it is nested in.
fn static_field(
    u: &mut Universe,
    sources: &JavaSources,
    at: &Located,
    path: &[String],
) -> Result<SymbolId, String> {
    let refused = || format!("`{}` as Java annotation argument", path.join("."));
    let (name, classes) = match path.split_last() {
        Some((name, [])) => (name, at.enclosing()),
        Some((name, class)) => {
            let names: Vec<&str> = class.iter().map(String::as_str).collect();
            let scope = Scope { at, vars: Vec::new(), in_header: false };
            let Ok(Resolved::Class(c)) = resolve(u, sources, &scope, &names) else { return Err(refused()) };
            (name, vec![c])
        }
        None => return Err(refused()),
    };
    for internal in classes {
        let class = class_symbol(u, &internal);
        let statics = class.and_then(|c| u.companion_module(c)).and_then(|m| u.module_class(m));
        let Some(statics) = statics else { continue };
        let field = u
            .symbol(statics)
            .decls
            .iter()
            .copied()
            .find(|&d| u.symbol(d).kind == Kind::Val && u.symbol(d).name == *name);
        if let Some(field) = field {
            return Ok(field);
        }
    }
    Err(refused())
}
