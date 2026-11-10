// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/Extractors.scala
//! Types as `Printer.TypeReprStructure` shows them (dotty's `Extractors.visitType`).

use super::literals::constant_structure;
use super::printed::Printed;
use crate::host::Host;
use syntax::ast::constant::Constant as C;
use tasty::{Const, Kind, SymbolId, Type, TypeId, constant};

const STRUCTURE: &str = "Printer.TypeReprStructure of a type the frontend does not print";

/// The `ThisType` of the package class named `name`.
fn package_this(name: &str) -> String {
    format!("ThisType(TypeRef(NoPrefix(), \"{name}\"))")
}

impl Host {
    /// Type `t` as `Printer.TypeReprStructure` shows it.
    pub fn type_structure(&mut self, t: TypeId) -> Result<Printed, &'static str> {
        let mut out = Printed::default();
        self.visit_type(t, &mut out)?;
        Ok(out)
    }

    fn visit_type(&mut self, t: TypeId, out: &mut Printed) -> Result<(), &'static str> {
        match self.u.ty(t).clone() {
            Type::Constant(tag, bits) => {
                let c = match constant(tag, bits).ok_or(STRUCTURE)? {
                    Const::Unit => C::Unit,
                    Const::Bool(b) => C::Boolean(b),
                    Const::Null => C::Null,
                    Const::Byte(n) => C::Byte(n),
                    Const::Short(n) => C::Short(n),
                    Const::Char(_) => C::Char(u16::try_from(bits).map_err(|_| STRUCTURE)?),
                    Const::Int(n) => C::Int(n),
                    Const::Long(n) => C::Long(n),
                    Const::Float(x) => C::Float(x),
                    Const::Double(x) => C::Double(x),
                    Const::Str(_) => return Err(STRUCTURE),
                };
                out.push_str("ConstantType(");
                constant_structure(&c, out)?;
                out.push(')');
            }
            Type::Str(s) => {
                out.push_str("ConstantType(");
                constant_structure(&C::String(s.encode_utf16().collect()), out)?;
                out.push(')');
            }
            Type::Sym(prefix, s) => {
                let sym = self.u.symbol(s);
                let (term, name) = (sym.kind.is_term(), sym.name.clone());
                if sym.kind == Kind::Package {
                    return self.package_ref(s, out);
                }
                let prefix = match prefix {
                    Some(p) => p,
                    None => self.qualifier(s)?,
                };
                self.reference(term, prefix, &name, out)?;
            }
            Type::TypeRef(p, n) => self.reference(false, p, &n, out)?,
            Type::TermRef(p, n) => self.reference(true, p, &n, out)?,
            Type::Package(p) => self.package_ref(p, out)?,
            Type::This(inner) => match self.package_class(inner) {
                Some(pkg) => out.push_str(&package_this(&pkg)),
                None => self.node(out, "ThisType", &[inner], None)?,
            },
            Type::NoPrefix => out.push_str("NoPrefix()"),
            Type::Applied(tycon, args) => self.node(out, "AppliedType", &[tycon], Some(&args))?,
            Type::Refined(parent, name, info) => {
                out.push_str("Refinement(");
                self.visit_type(parent, out)?;
                out.push_str(&format!(", \"{name}\", "));
                self.visit_type(info, out)?;
                out.push(')');
            }
            Type::And(l, r) => self.node(out, "AndType", &[l, r], None)?,
            Type::Or(l, r, _) => self.node(out, "OrType", &[l, r], None)?,
            Type::Bounds(lo, hi) => self.node(out, "TypeBounds", &[lo, hi], None)?,
            Type::Alias(a) => self.node(out, "TypeBounds", &[a, a], None)?,
            Type::ByName(u) => self.node(out, "ByNameType", &[u], None)?,
            Type::Super(this, sup) => self.node(out, "SuperType", &[this, sup], None)?,
            Type::MatchCase(pat, rhs) => self.node(out, "MatchCase", &[pat, rhs], None)?,
            Type::Match { bound, scrutinee, cases } => {
                self.node(out, "MatchType", &[bound, scrutinee], Some(&cases))?
            }
            Type::Param(_, i) => out.push_str(&format!("ParamRef(binder, {i})")),
            Type::Method { params, result, .. } => self.lambda(out, "MethodType", &params, result)?,
            Type::Poly { params, result, .. } => self.lambda(out, "PolyType", &params, result)?,
            Type::Lambda { params, result, .. } => self.lambda(out, "TypeLambda", &params, result)?,
            Type::Annotated(inner, annot) if self.is_synthetic_annotation(annot) => {
                out.push_str("AnnotatedType(");
                self.visit_type(inner, out)?;
                out.push_str(", Apply(Select(New(Inferred()), \"<init>\"), Nil))");
            }
            _ => return Err(STRUCTURE),
        }
        Ok(())
    }

    /// Whether `annot` is the class of an annotation the compiler adds, of `scala.annotation.internal`,
    /// whose constructor takes no arguments.
    fn is_synthetic_annotation(&mut self, annot: TypeId) -> bool {
        let Some(c) = self.u.class_symbol(annot) else { return false };
        let internal =
            self.u.symbol(c).owner.is_some_and(|o| self.u.full_name(o) == "scala.annotation.internal");
        let Some(init) = self.primary_constructor(c).filter(|_| internal) else { return false };
        let info = self.u.info(init);
        self.u.method(info).is_some_and(|m| m.params.is_empty() && self.u.method(m.result).is_none())
    }

    /// The `TermRef` of package `p` (dotty's `p.termRef`, whose prefix is the owner's `this`); the
    /// root's as the empty package's `this`.
    fn package_ref(&mut self, p: SymbolId, out: &mut Printed) -> Result<(), &'static str> {
        if p == self.u.root() {
            out.push_str(&package_this("<empty>"));
            return Ok(());
        }
        let owner = self.u.symbol(p).owner.ok_or(STRUCTURE)?;
        let owner = match owner == self.u.root() {
            true => "<root>",
            false => &self.u.symbol(owner).name,
        };
        let name = &self.u.symbol(p).name;
        out.push_str(&format!("TermRef({}, \"{name}\")", package_this(owner)));
        Ok(())
    }

    /// The name of the class of the package `t` refers to, `<empty>` for the root.
    fn package_class(&mut self, t: TypeId) -> Option<String> {
        let p = match self.u.ty(t).clone() {
            Type::Package(p) => p,
            Type::Sym(_, p) if self.u.symbol(p).kind == Kind::Package => p,
            Type::TermRef(..) => self.u.resolve_ref(t).filter(|&p| self.u.symbol(p).kind == Kind::Package)?,
            Type::PackageName(n) => return Some(n.rsplit('.').next().unwrap_or(&n).to_owned()),
            _ => return None,
        };
        Some(match p == self.u.root() {
            true => "<empty>".to_owned(),
            false => self.u.symbol(p).name.clone(),
        })
    }

    /// `TermRef(prefix, "name")` if `term`, else `TypeRef(prefix, "name")`.
    fn reference(
        &mut self,
        term: bool,
        prefix: TypeId,
        name: &str,
        out: &mut Printed,
    ) -> Result<(), &'static str> {
        out.push_str(if term { "TermRef(" } else { "TypeRef(" });
        self.visit_type(prefix, out)?;
        out.push_str(&format!(", \"{name}\")"));
        Ok(())
    }

    /// `what(` the types `types`, then the list `list`, `)`.
    fn node(
        &mut self,
        out: &mut Printed,
        what: &str,
        types: &[TypeId],
        list: Option<&[TypeId]>,
    ) -> Result<(), &'static str> {
        out.push_str(what);
        out.push('(');
        for (i, &t) in types.iter().enumerate() {
            out.push_str(if i > 0 { ", " } else { "" });
            self.visit_type(t, out)?;
        }
        if let Some(list) = list {
            out.push_str(if types.is_empty() { "" } else { ", " });
            self.type_list(list, out)?;
        }
        out.push(')');
        Ok(())
    }

    fn lambda(
        &mut self,
        out: &mut Printed,
        what: &str,
        params: &[(String, TypeId)],
        result: TypeId,
    ) -> Result<(), &'static str> {
        let names: Vec<&str> = params.iter().map(|(n, _)| n.as_str()).collect();
        match names.is_empty() {
            true => out.push_str(&format!("{what}(Nil, ")),
            false => out.push_str(&format!("{what}(List({}), ", names.join(", "))),
        }
        let types: Vec<TypeId> = params.iter().map(|&(_, t)| t).collect();
        self.type_list(&types, out)?;
        out.push_str(", ");
        self.visit_type(result, out)?;
        out.push(')');
        Ok(())
    }

    fn type_list(&mut self, types: &[TypeId], out: &mut Printed) -> Result<(), &'static str> {
        if types.is_empty() {
            out.push_str("Nil");
            return Ok(());
        }
        out.push_str("List(");
        for (i, &t) in types.iter().enumerate() {
            out.push_str(if i > 0 { ", " } else { "" });
            self.visit_type(t, out)?;
        }
        out.push(')');
        Ok(())
    }
}
