//! String concatenation: the operands of `a + b + …` appended to a `StringBuilder` (nscplugin's
//! `genStringConcat`, `genStringBuilderAppend`).

use super::Gen;
use crate::{Result, unsupported};
use nir::{Op, Type, Val, ValId};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::SymbolId;
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
            && self.literal(first) == Some(Constant::String(Box::new([])))
        {
            return self.gen_apply_static(value_of, Some(string), &[arg], span);
        }
        let parts: Vec<TNodeId> = parts
            .into_iter()
            .filter(|&p| self.literal(p) != Some(Constant::String(Box::new([]))))
            .map(|p| self.unboxed(p))
            .collect();
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
        let tpe = self.l.u.widen(self.l.tpd.tpe(t));
        let class = self.l.u.class_symbol(tpe);
        let s = &self.l.defs.strings;
        let as_class = [s.string, s.string_buffer, s.char_sequence]
            .into_iter()
            .find(|&c| class.is_some_and(|k| self.l.u.derives_from(k, c)));
        let erased = self.gen_type(tpe);
        let mut arg_ty = match (as_class, self.pool.ty(erased)) {
            (_, Type::Null) => self.class_ref(self.l.defs.strings.string),
            (Some(c), _) => self.class_ref(c),
            (None, Type::Ref { .. } | Type::Array { .. }) => self.class_ref(self.l.defs.object),
            (None, _) => erased,
        };
        let mut value = self.gen_expr(t)?;
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

    fn literal(&self, t: TNodeId) -> Option<Constant> {
        match self.l.tpd.tree(t) {
            Tree::Literal { value } => Some(value.clone()),
            _ => None,
        }
    }

    /// The length of literal `t` as a string; 0 for another tree or `null`.
    fn literal_length(&self, t: TNodeId) -> usize {
        match self.literal(t) {
            Some(Constant::Null) | None => 0,
            Some(c) => c.string_value().len(),
        }
    }
}
