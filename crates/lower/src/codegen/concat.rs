//! String concatenation: the operands of `a + b + …` appended to a `StringBuilder` (nscplugin's
//! `genStringConcat`, `genStringBuilderAppend`).

use super::Gen;
use crate::phases::adapt::erased_type;
use crate::{Result, unsupported};
use nir::{Op, Type, Val, ValId};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::{Erased, SymbolId};
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// Whether `sym` is `String`'s `+`.
    pub fn is_string_concat(&self, sym: SymbolId) -> bool {
        let s = self.l.u.symbol(sym);
        s.name == "+" && s.owner == Some(self.l.defs.strings.string)
    }

    pub fn gen_string_concat(&mut self, app: TNodeId, span: Span) -> Result<ValId> {
        let parts = self.lift_string_concat(app);
        let (string, value_of) = (self.l.defs.strings.string, self.l.defs.strings.value_of);
        if let [first, arg] = parts[..]
            && self.is_empty_string(first)
        {
            return self.gen_apply_static(value_of, Some(string), &[arg], span);
        }
        let parts: Vec<TNodeId> =
            parts.into_iter().filter(|&p| !self.is_empty_string(p)).map(|p| self.unboxed(p)).collect();
        let size = parts.iter().map(|&p| self.literal_length(p)).sum::<usize>();
        let (builder, ctor) = (self.l.defs.strings.builder, self.l.defs.strings.builder_ctor);
        let name = self.type_name(builder);
        let sb = self.emit(Op::Classalloc { name, zone: None });
        let size = self.val(Val::Int(size as i32));
        self.call_method(ctor, true, sb, vec![size]);
        for p in parts {
            self.gen_string_builder_append(sb, p)?;
        }
        Ok(self.call_method(self.l.defs.strings.builder_to_string, true, sb, Vec::new()))
    }

    /// `sb.append(t)` with the overload for `t`'s type.
    fn gen_string_builder_append(&mut self, sb: ValId, t: TNodeId) -> Result<()> {
        let span = self.l.tpd.span(t);
        let saved = self.at_span(span);
        let erased = erased_type(self.l, t);
        let class = match &erased {
            Erased::Class(name) => self.class_named(t, name),
            _ => None,
        };
        let s = &self.l.defs.strings;
        let as_class = [s.string, s.string_buffer, s.char_sequence]
            .into_iter()
            .find(|&c| class.is_some_and(|k| self.l.u.derives_from(k, c)));
        let mut arg_ty = match (as_class, &erased) {
            (_, Erased::Null | Erased::Nothing) => self.class_ref(self.l.defs.strings.string),
            (Some(c), _) => self.class_ref(c),
            (None, Erased::Class(_) | Erased::Array(_)) => self.class_ref(self.l.defs.object),
            (None, e) => self.erased(e),
        };
        let mut value = self.gen_expr(t)?;
        self.check_conforms(value, arg_ty, span)?;
        if matches!(self.pool.ty(arg_ty), Type::Byte | Type::Short) {
            let int = self.ty(Type::Int);
            value = self.coerce(value, arg_ty, int);
            arg_ty = int;
        }
        let appends = self.l.defs.strings.appends.clone();
        let Some(append) = appends.into_iter().find(|&m| self.param_types(m) == [arg_ty]) else {
            return unsupported(span, "concatenation of a value without an `append` overload");
        };
        self.call_method(append, true, sb, vec![value]);
        self.m.pos = saved;
        Ok(())
    }

    /// The class of binary name `name`: that of `t`'s type if it has that name, else the
    /// top-level class of that name.
    fn class_named(&mut self, t: TNodeId, name: &str) -> Option<SymbolId> {
        let tpe = self.l.u.widen(self.l.tpd.tpe(t));
        if let Some(c) = self.l.u.class_symbol(tpe)
            && self.l.u.binary_name(c) == name
        {
            return Some(c);
        }
        let named = self.l.u.class_ref(name);
        self.l.u.class_symbol(named)
    }

    /// The operands of the concatenations `t` is a chain of, from left to right.
    fn lift_string_concat(&self, t: TNodeId) -> Vec<TNodeId> {
        if let Tree::Apply { fun, args } = *self.l.tpd.tree(t)
            && let Tree::Select { qualifier, .. } = *self.l.tpd.tree(fun)
            && self.l.tpd.sym(fun).is_some_and(|s| self.is_string_concat(s))
        {
            let mut parts = self.lift_string_concat(qualifier);
            parts.extend(self.l.tpd.items(args));
            return parts;
        }
        vec![t]
    }

    /// The value `t` boxes if it is a call of a primitive's `box`, else `t`.
    fn unboxed(&self, t: TNodeId) -> TNodeId {
        if let Tree::Apply { fun, args } = *self.l.tpd.tree(t)
            && self.l.tpd.sym(fun).is_some_and(|s| self.l.defs.is_box(s))
            && let [value] = self.l.tpd.items(args)
        {
            return *value;
        }
        t
    }

    fn is_empty_string(&self, t: TNodeId) -> bool {
        matches!(self.l.tpd.tree(t), Tree::Literal { value: Constant::String(s) } if s.is_empty())
    }

    /// The length of literal `t` as a string; 0 for another tree or `null`.
    fn literal_length(&self, t: TNodeId) -> usize {
        match self.l.tpd.tree(t) {
            Tree::Literal { value: Constant::String(s) } => s.len(),
            Tree::Literal { value: Constant::Null } => 0,
            Tree::Literal { value } => value.string_value().len(),
            _ => 0,
        }
    }
}
