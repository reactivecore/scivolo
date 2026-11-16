// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! The elements of a Java annotation a class file keeps (dotty's `assocsFromApply`,
//! `emitArgument`).

use super::Gen;
use super::descriptors::{class_descriptor, class_internal, descriptor};
use classfile::{AnnotationValue, ElementValue};
use names::nme;
use syntax::ast::constant::Constant;
use tasty::tags::CLASSconst;
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The refusal of an annotation argument the backend does not write.
const UNSUPPORTED: &str = "Java annotation arguments other than constants, enum constants, class literals, arrays and annotations on the JVM";

impl Gen<'_, '_> {
    /// Determines the class-file form of annotation `annot` of a class file's annotation class:
    /// its descriptor and its elements in the constructor's parameter order, those whose argument
    /// is the placeholder `_` of a default value left out. Refuses an argument that is no
    /// constant, Java enum constant, class literal, array of these or annotation.
    pub fn annotation_value(&mut self, annot: TNodeId) -> Result<AnnotationValue, &'static str> {
        let tpe = self.l.tpd.tpe(annot);
        let descriptor = class_descriptor(&class_internal(&self.l.u.erase(tpe)));
        let (fun, args) = match *self.l.tpd.tree(annot) {
            Tree::Apply { fun, args } => (fun, args),
            Tree::New { .. } => return Ok(AnnotationValue { descriptor, elements: Vec::new() }),
            _ => return Err(UNSUPPORTED),
        };
        let ctor = self.l.tpd.sym(fun).ok_or(UNSUPPORTED)?;
        let info = self.l.u.info(ctor);
        let names = self.l.u.method(info).ok_or(UNSUPPORTED)?.params.iter().map(|&(n, _)| n);
        let names: Vec<String> = names.map(|n| self.l.u.name(n).to_owned()).collect();
        let mut elements = Vec::new();
        for (name, &arg) in names.into_iter().zip(self.l.tpd.items(args).to_vec().iter()) {
            if !self.is_default_placeholder(arg) {
                elements.push((name, self.element_value(arg)?));
            }
        }
        Ok(AnnotationValue { descriptor, elements })
    }

    /// Whether `arg` is the placeholder `_` the typer gives an omitted argument with a default.
    fn is_default_placeholder(&self, arg: TNodeId) -> bool {
        let arg = self.normalized(arg);
        matches!(*self.l.tpd.tree(arg), Tree::Ident { .. }) && self.l.tpd.sym(arg).is_none()
    }

    /// `arg` without the named argument and type ascription around it (dotty's
    /// `normalizeArgument`).
    fn normalized(&self, mut arg: TNodeId) -> TNodeId {
        loop {
            arg = match *self.l.tpd.tree(arg) {
                Tree::NamedArg { arg, .. } => arg,
                Tree::Typed { expr, .. } => expr,
                _ => return arg,
            };
        }
    }

    fn element_value(&mut self, arg: TNodeId) -> Result<ElementValue, &'static str> {
        let arg = self.normalized(arg);
        if let Type::Constant(tag, class) = *self.l.u.ty(self.l.tpd.tpe(arg))
            && tag == CLASSconst
        {
            let class = TypeId(class as u32);
            return Ok(ElementValue::Class(descriptor(&self.l.u.erase(class))));
        }
        match *self.l.tpd.tree(arg) {
            Tree::Literal { ref value } => constant(value),
            Tree::ClassLiteral { tpe } => Ok(ElementValue::Class(descriptor(&self.l.u.erase(tpe)))),
            Tree::SeqLiteral { elems, .. } => {
                let elems = self.l.tpd.items(elems).to_vec();
                Ok(ElementValue::Array(
                    elems.into_iter().map(|e| self.element_value(e)).collect::<Result<_, _>>()?,
                ))
            }
            Tree::Ident { .. } | Tree::Select { .. } => {
                let sym = self.l.tpd.sym(arg).ok_or(UNSUPPORTED)?;
                let constant = self.aliased_enum_constant(sym).unwrap_or(sym);
                let class = self.java_enum_of(constant).ok_or(UNSUPPORTED)?;
                let descriptor = class_descriptor(&self.class_name(class));
                Ok(ElementValue::Enum(descriptor, self.l.u.symbol(constant).name.clone()))
            }
            Tree::Apply { fun, args } => match self.array_apply_args(fun, args)? {
                Some(elems) => {
                    let values = elems.into_iter().map(|e| self.element_value(e));
                    Ok(ElementValue::Array(values.collect::<Result<_, _>>()?))
                }
                None => {
                    let c = self.l.u.class_symbol(self.l.tpd.tpe(arg)).ok_or(UNSUPPORTED)?;
                    self.l.u.classfile_retention(c).ok_or(UNSUPPORTED)?;
                    Ok(ElementValue::Annotation(self.annotation_value(arg)?))
                }
            },
            _ => Err(UNSUPPORTED),
        }
    }

    /// The Java enum class whose constant `sym` is: a value of an object whose class derives from
    /// `java.lang.Enum` (dotty's `JavaEnum` check on the linked class).
    fn java_enum_of(&mut self, sym: SymbolId) -> Option<SymbolId> {
        let statics = self.l.u.symbol(sym).owner?;
        let class = self.l.u.companion_class(statics)?;
        let value = self.l.u.symbol(sym).kind == Kind::Val;
        (value && lower::derives_from_java_enum(self.l, class)).then_some(class)
    }

    /// The Java enum constant whose singleton type is the result type of `sym`, an alias such as
    /// `final val R = RetentionPolicy.RUNTIME` (dotty's `emitArgument`).
    fn aliased_enum_constant(&mut self, sym: SymbolId) -> Option<SymbolId> {
        let info = self.l.u.info(sym);
        let result = self.l.u.result_type(info);
        let constant = match *self.l.u.ty(result) {
            Type::Sym(_, s) => s,
            Type::TermRef(..) => self.l.u.resolve_ref(result)?,
            _ => return None,
        };
        self.java_enum_of(constant).is_some().then_some(constant)
    }

    /// The elements of call `fun(args)` if it applies `Array.apply`, its implicit `ClassTag`
    /// clause skipped and repeated arguments flattened.
    fn array_apply_args(
        &mut self,
        fun: TNodeId,
        args: typer::tpd::List,
    ) -> Result<Option<Vec<TNodeId>>, &'static str> {
        let (fun, args) = match *self.l.tpd.tree(fun) {
            Tree::Apply { fun: inner, args: values } => match self.is_implicit_method(fun) {
                true => (inner, values),
                false => (fun, args),
            },
            _ => (fun, args),
        };
        let Some(method) = self.l.tpd.sym(self.l.tpd.meth_part(fun)) else { return Ok(None) };
        let array = self.l.u.defn().array(self.l.u).map_err(|_| UNSUPPORTED)?;
        let array_module = self.l.u.companion_module(array).and_then(|m| self.l.u.module_class(m));
        let s = self.l.u.symbol(method);
        if s.name != nme::APPLY || s.owner.is_none() || s.owner != array_module {
            return Ok(None);
        }
        let mut elems = Vec::new();
        for arg in self.l.tpd.items(args).to_vec() {
            let arg = self.normalized(arg);
            match *self.l.tpd.tree(arg) {
                Tree::SeqLiteral { elems: seq, .. } => elems.extend(self.l.tpd.items(seq).iter().copied()),
                _ => elems.push(arg),
            }
        }
        Ok(Some(elems))
    }

    /// Whether `fun`'s type is a method of an implicit clause.
    fn is_implicit_method(&mut self, fun: TNodeId) -> bool {
        let tpe = self.l.tpd.tpe(fun);
        self.l.u.method(tpe).is_some_and(|m| m.implicit)
    }
}

/// The element value of literal `c`; refuses `null`, `()` and a string with a lone surrogate.
fn constant(c: &Constant) -> Result<ElementValue, &'static str> {
    Ok(match c {
        Constant::Boolean(b) => ElementValue::Boolean(*b),
        Constant::Byte(b) => ElementValue::Byte(*b),
        Constant::Short(s) => ElementValue::Short(*s),
        Constant::Char(c) => ElementValue::Char(*c),
        Constant::Int(i) => ElementValue::Int(*i),
        Constant::Long(l) => ElementValue::Long(*l),
        Constant::Float(f) => ElementValue::Float(*f),
        Constant::Double(d) => ElementValue::Double(*d),
        Constant::String(s) => ElementValue::String(String::from_utf16(s).map_err(|_| UNSUPPORTED)?),
        Constant::Unit | Constant::Null => return Err(UNSUPPORTED),
    })
}
