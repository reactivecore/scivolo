// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenName.scala
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenStat.scala
//! Fields: a `var` per field of a class and a static getter per static one, loads and stores
//! (nscplugin's `genClassFields`, `genFieldName`, and the field cases of `genSelect` and
//! `genAssign`).

use super::Gen;
use crate::symbols::{has, is_static_module};
use crate::{Result, unsupported};
use nir::sig::{Scope, Sig};
use nir::{Attr, Defn, DefnKind, GlobalId, Op, PosId, Val, ValId};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::SymbolId;
use tasty::tags::{FIELDaccessor, MUTABLE, PRIVATE, STATIC};
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// Whether `s` is a field: one `Memoize` made, a private local value, or a Java field.
    pub fn is_field(&self, s: SymbolId) -> bool {
        self.l.fields.contains_key(&s) || self.l.u.is_java_field(s)
    }

    /// A `var` for each field of class `c` with template `template`, in the order of their
    /// definitions there, then of declaration.
    pub fn gen_class_fields(&mut self, c: SymbolId, template: TNodeId, class_pos: PosId) {
        let decls = self.l.u.decls(c).unwrap_or_default();
        let mut fields: Vec<SymbolId> = decls.into_iter().filter(|&f| self.is_field(f)).collect();
        let stats = match *self.l.tpd.tree(template) {
            Tree::Template { body, .. } => self.l.tpd.items(body).to_vec(),
            _ => Vec::new(),
        };
        let defined: Vec<SymbolId> = stats.iter().filter_map(|&s| self.l.tpd.sym(s)).collect();
        fields.sort_by_key(|f| defined.iter().position(|d| d == f).unwrap_or(usize::MAX));
        for f in fields {
            let pos = self.sym_pos(f, c);
            let pos = if pos == self.no_pos { class_pos } else { pos };
            let mutable = has(self.l.u, f, MUTABLE);
            let attrs = match () {
                _ if self.l.lazy_fields.iter().any(|lazy| lazy.field == f) => vec![Attr::Volatile],
                _ if mutable => Vec::new(),
                _ => vec![Attr::Final],
            };
            let attrs = self.field_attrs(f, c, attrs);
            let info = self.l.u.info(f);
            let ty = self.gen_type(info);
            let rhs = self.val(Val::Zero(ty));
            let name = self.field_name(f);
            self.defns.push(Defn { name, attrs, pos, kind: DefnKind::Var { ty, rhs } });
            if has(self.l.u, f, STATIC) {
                let getter = self.static_getter(f, c, ty, pos);
                self.defns.push(getter);
            }
        }
    }

    pub fn field_name(&mut self, f: SymbolId) -> GlobalId {
        let owner = self.l.u.symbol(f).owner.expect("a field's class");
        let owner = self.type_name(owner);
        let scope = if has(self.l.u, f, PRIVATE) && !has(self.l.u, f, MUTABLE) {
            Scope::Private(owner)
        } else {
            Scope::Public
        };
        let id = self.local_name(f);
        self.pool.member(owner, &Sig::Field(id, scope))
    }

    /// The value of field `f` of `qual`, or of `this` for an identifier.
    pub fn gen_field_load(&mut self, t: TNodeId, f: SymbolId, qual: Option<TNodeId>) -> Result<ValId> {
        if has(self.l.u, f, STATIC) {
            return self.gen_apply_static(f, None, &[], self.l.tpd.span(t));
        }
        let ty = self.gen_type(self.l.tpd.tpe(t));
        let obj = self.field_owner(f, qual, self.l.tpd.span(t))?;
        let name = self.field_name(f);
        Ok(self.emit(Op::Fieldload { ty, obj, name }))
    }

    /// The address of the field selection `t` (nscplugin's `classFieldRawPtr`).
    pub fn gen_field_ptr(&mut self, t: TNodeId) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let (Tree::Select { qualifier, .. }, Some(f)) = (self.l.tpd.tree(t).clone(), self.l.tpd.sym(t))
        else {
            return unsupported(span, "the address of a value that is not a field");
        };
        let obj = self.gen_expr(qualifier)?;
        let name = self.field_name(f);
        Ok(self.emit(Op::Field { obj, name }))
    }

    /// `classFieldRawPtr(target, "name")`: the address of mutable field `name` of `target`
    /// (nscplugin's `genClassFieldRawPtr`).
    pub fn gen_class_field_raw_ptr(&mut self, target: TNodeId, field: TNodeId, span: Span) -> Result<ValId> {
        let Tree::Literal { value: Constant::String(units) } = self.l.tpd.tree(field) else {
            return unsupported(span, "`classFieldRawPtr` of a field name that is no literal");
        };
        let name = String::from_utf16_lossy(units);
        let tpe = self.l.u.widen(self.l.tpd.tpe(target));
        let Some(class) = self.l.u.class_symbol(tpe) else {
            return unsupported(span, "`classFieldRawPtr` of a value that is no class instance");
        };
        let mut bases = self.l.u.linearization(class);
        if bases.first() != Some(&class) {
            bases.insert(0, class);
        }
        let mut found = None;
        for &base in &bases {
            let decls = self.l.u.decls(base).unwrap_or_default();
            let named: Vec<SymbolId> =
                decls.into_iter().filter(|&d| self.l.u.symbol(d).name == name).collect();
            found = found.or_else(|| named.iter().copied().find(|&d| self.is_field(d)));
            let immutable_accessor = base != class
                && named.iter().any(|&d| has(self.l.u, d, FIELDaccessor) && !has(self.l.u, d, MUTABLE));
            if immutable_accessor {
                return unsupported(span, format!("the address of immutable field {name}"));
            }
        }
        let Some(f) = found else {
            return unsupported(span, format!("`classFieldRawPtr` of {name}, no field of the unit"));
        };
        if !has(self.l.u, f, MUTABLE) {
            return unsupported(span, format!("the address of immutable field {name}"));
        }
        let obj = self.gen_expr(target)?;
        let name = self.field_name(f);
        Ok(self.emit(Op::Field { obj, name }))
    }

    /// The object holding field `f`: `qual` if a `this`, a static object's instance, else `qual`
    /// or `this`.
    fn field_owner(&mut self, f: SymbolId, qual: Option<TNodeId>, span: Span) -> Result<ValId> {
        let owner = self.l.u.symbol(f).owner.expect("a field's class");
        match qual {
            _ if has(self.l.u, f, STATIC) => Ok(self.gen_module(owner)),
            Some(q) if matches!(self.l.tpd.tree(q), Tree::This { .. }) => self.gen_expr(q),
            _ if is_static_module(self.l.u, owner) => Ok(self.gen_module(owner)),
            Some(q) => self.gen_expr(q),
            None => self.this_of(owner, span),
        }
    }

    /// `lhs = rhs` for a field `lhs`.
    pub fn gen_assign(&mut self, t: TNodeId, lhs: TNodeId, rhs: TNodeId) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let (qualifier, Some(f)) = (self.l.tpd.tree(lhs).clone(), self.l.tpd.sym(lhs)) else {
            return unsupported(span, "an assignment without a symbol");
        };
        let qualifier = match qualifier {
            Tree::Select { qualifier, .. } => Some(qualifier),
            _ => None,
        };
        if let Some(&slot) = self.m.env.get(&f).filter(|_| self.m.mutable.contains(&f)) {
            let saved = self.at_span(span);
            let value = self.gen_expr(rhs)?;
            let ty = self.gen_type(self.l.tpd.tpe(lhs));
            self.check_conforms(value, ty, span)?;
            let v = self.emit(Op::Varstore { slot, value });
            self.m.pos = saved;
            return Ok(v);
        }
        if !self.is_field(f) {
            return unsupported(span, "assignments through setters");
        }
        let saved = self.at_span(span);
        let obj = self.field_owner(f, qualifier, span)?;
        let ty = self.gen_type(self.l.tpd.tpe(lhs));
        let value = self.gen_expr(rhs)?;
        self.check_conforms(value, ty, span)?;
        let name = self.field_name(f);
        let v = self.emit(Op::Fieldstore { ty, obj, name, value });
        self.m.pos = saved;
        Ok(v)
    }
}
