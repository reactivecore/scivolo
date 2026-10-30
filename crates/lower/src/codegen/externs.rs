// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenStat.scala
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenType.scala
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenName.scala
//! Extern objects and traits: their declarations, names and C signatures (nscplugin's
//! `genExternMethod`, `genExternMethodSig`, `genExternType`).

use super::Gen;
use crate::symbols::is_extern;
use crate::{Result, unsupported};
use nir::sig::Sig;
use nir::{Attr, Defn, DefnKind, GlobalId, PosId, Type, TypeId};
use tasty::{Erased, SymbolId};
use typer::tpd::{TNodeId, Template, Tree};

const EXTERN_BODY: &str = "scala.scalanative.unsafe.package$.extern";
const NAME: &str = "scala.scalanative.unsafe.name";
const BLOCKING: &str = "scala.scalanative.unsafe.package$.blocking";

/// The arity of function pointer class `name`.
fn func_arity(name: &str) -> Option<u8> {
    name.strip_prefix("scala.scalanative.unsafe.CFuncPtr")?.parse().ok()
}

/// The C type a value of class `name` passes as (NIR's `Type.unbox`, without `CFuncPtr22`).
pub fn unboxed(name: &str) -> Option<Type> {
    let func = func_arity(name).is_some_and(|n| n < 22);
    Some(match name {
        _ if func => Type::Ptr,
        "scala.scalanative.unsafe.Ptr" | "scala.scalanative.unsafe.CArray" => Type::Ptr,
        "scala.scalanative.unsafe.CVarArgList" => Type::Ptr,
        "scala.scalanative.unsafe.Size" | "scala.scalanative.unsigned.USize" => Type::Size,
        "scala.scalanative.unsigned.UByte" | "java.lang.Byte" => Type::Byte,
        "scala.scalanative.unsigned.UShort" | "java.lang.Short" => Type::Short,
        "scala.scalanative.unsigned.UInt" | "java.lang.Integer" => Type::Int,
        "scala.scalanative.unsigned.ULong" | "java.lang.Long" => Type::Long,
        "java.lang.Boolean" => Type::Bool,
        "java.lang.Character" => Type::Char,
        "java.lang.Float" => Type::Float,
        "java.lang.Double" => Type::Double,
        _ => return None,
    })
}

impl Gen<'_, '_> {
    /// An extern object or trait: its declarations and its methods that are not extern, without
    /// constructor, fields or forwarders.
    pub fn gen_extern_class(&mut self, td: TNodeId, sym: SymbolId, pos: PosId) -> Result<()> {
        let span = self.l.tpd.span(td);
        let blocking = self.is_blocking(sym);
        let attrs = self.class_attrs(sym, Vec::new());
        let attrs = attrs.into_iter().map(|a| if a.is_extern() { Attr::Extern { blocking } } else { a });
        let attrs = attrs.collect();
        let name = self.type_name(sym);
        let traits = self.class_interfaces(sym);
        let kind = if crate::symbols::is_static_module(self.l.u, sym) {
            DefnKind::Module { parent: Some(self.pool.top("java.lang.Object")), traits }
        } else if self.l.u.is_trait(sym) {
            DefnKind::Trait { traits }
        } else {
            return unsupported(span, "extern classes that are no object or trait");
        };
        self.defns.push(Defn { name, attrs, pos, kind });
        let Some(Template { constr, body, .. }) = self.l.tpd.class_template(td) else {
            return unsupported(span, "a type definition that is not a class");
        };
        if self.l.u.is_trait(sym) {
            let pos = self.pos_or(self.l.tpd.span(constr), pos);
            let name = self.pool.member(name, &Sig::Extern("$init$".into()));
            let unit = self.ty(Type::Unit);
            let ty = self.ty(Type::Function(Vec::new(), unit));
            let attrs = vec![Attr::Extern { blocking: false }];
            self.defns.push(Defn { name, attrs, pos, kind: DefnKind::Declare { ty } });
        } else {
            self.check_extern_ctor(constr)?;
        }
        for &stat in self.l.tpd.items(body).to_vec().iter() {
            let is_extern = self.l.tpd.sym(stat).is_some_and(|s| is_extern(self.l.u, s));
            match self.l.tpd.tree(stat) {
                Tree::DefDef { .. } if !is_extern => {
                    let defn = self.gen_method(stat)?;
                    self.defns.push(defn);
                }
                Tree::DefDef { .. } => {
                    let defn = self.gen_extern_method(stat)?;
                    self.defns.extend(defn);
                }
                Tree::TypeDef { .. } | Tree::Import { .. } => {}
                Tree::ValDef { .. } => self.gen_extern_field(stat, sym, pos)?,
                _ => return unsupported(self.l.tpd.span(stat), "statements in an extern object"),
            }
        }
        self.gen_mirror_class(sym, pos)
    }

    pub fn is_blocking(&mut self, sym: SymbolId) -> bool {
        self.l.u.is_annotated(sym, BLOCKING)
    }

    /// Whether class `c` is an extern object or trait (nscplugin's `isExternType`).
    pub fn is_extern_type(&mut self, c: SymbolId) -> bool {
        let annotated = self.l.u.symbol(c).is_extern || self.l.u.is_annotated(c, crate::symbols::EXTERN);
        annotated && (self.l.u.is_module_class(c) || self.l.u.is_trait(c))
    }

    /// Whether `t` is `extern` or `extern()`.
    pub fn is_extern_body(&self, t: TNodeId) -> bool {
        let t = match *self.l.tpd.tree(t) {
            Tree::Apply { fun, args } if self.l.tpd.items(args).is_empty() => fun,
            _ => t,
        };
        self.l.tpd.sym(t).is_some_and(|s| self.l.u.full_name(s) == EXTERN_BODY)
    }

    /// The name in class `owner` of extern method `m`.
    pub fn extern_name(&mut self, owner: SymbolId, m: SymbolId, span: syntax::ast::Span) -> Result<GlobalId> {
        let owner = self.type_name(owner);
        let id = self.extern_id(m, span)?;
        Ok(self.pool.member(owner, &Sig::Extern(id)))
    }

    /// The C name of extern method `m`: its `@name`, else its simple name.
    pub fn extern_id(&mut self, m: SymbolId, span: syntax::ast::Span) -> Result<String> {
        if !self.l.u.is_annotated(m, NAME) {
            return Ok(self.l.u.symbol(m).name.clone());
        }
        let given = if self.l.u.symbol(m).tree.is_some() {
            self.l.u.annotation_string(m, NAME)
        } else {
            let class = self.l.u.annotations(m).iter().copied().find(|&c| self.l.u.full_name(c) == NAME);
            class.and_then(|c| self.l.u.annotation_strings(m, c).first().cloned().flatten())
        };
        given.map_or_else(|| unsupported(span, "@name without a string literal"), Ok)
    }

    /// The C signature of extern method `m`: unboxed parameters and result, no receiver.
    pub fn extern_sig(&mut self, m: SymbolId, span: syntax::ast::Span) -> Result<TypeId> {
        let (params, result) = self.l.u.erased_method(m);
        let vararg = self.extern_vararg_index(m);
        if vararg.is_some_and(|i| i + 1 != params.len()) {
            return unsupported(span, "extern methods with non-final variable arguments");
        }
        for (i, p) in params.iter().chain([&result]).enumerate() {
            if matches!(p, Erased::Class(n) if n == "scala.collection.immutable.Seq") && vararg != Some(i) {
                return unsupported(span, "extern methods with variable arguments");
            }
            if matches!(p, Erased::Class(n) if n == "scala.scalanative.unsafe.CFuncPtr") {
                return unsupported(span, "extern methods of a `CFuncPtr`");
            }
        }
        let args = params
            .iter()
            .enumerate()
            .map(|(i, p)| if vararg == Some(i) { self.ty(Type::Vararg) } else { self.extern_type(p) })
            .collect();
        let ret = self.extern_type(&result);
        Ok(self.ty(Type::Function(args, ret)))
    }

    pub fn extern_type(&mut self, e: &Erased) -> TypeId {
        match e {
            Erased::Class(name) if func_arity(name).is_some() => self.ty(Type::Ptr),
            Erased::Class(name) if let Some(t) = unboxed(name) => self.ty(t),
            e => self.erased(e),
        }
    }
}
