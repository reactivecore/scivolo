//! Constants in inline expansions: the constants trees denote by their types, the literals that
//! replace them, and the trees of singleton types (dotty's `ConstantValue`, `constToLiteral`,
//! `tpd.singleton`).

use super::Inlining;
use crate::tpd::{TNodeId, Tree as T, constant_type_of};
use syntax::ast::constant::Constant as C;
use syntax::ast::{Span, TName};
use tasty::tags::{INLINE, PARAMsetter, TRAIT};
use tasty::{Const, Kind, SymbolId, Type, TypeId, constant};

impl Inlining<'_> {
    /// `t` as a literal if its type is a constant and it is pure, or idempotent when it is an
    /// inline value; an impure unary operation or `getClass` call as its prefix followed by the
    /// literal (dotty's `constToLiteral`).
    pub fn const_to_literal(&mut self, t: TNodeId) -> TNodeId {
        let Some(tpe) = self.tpd.get(t).tpe else { return t };
        let Some(value) = self.constant_of_type(tpe) else { return t };
        let pre = self.dropped_op(t);
        let inline_val = self.tpd.sym(pre.unwrap_or(t)).is_some_and(|s| {
            let s = self.u.symbol(s);
            s.kind != Kind::Def && s.flags.has(INLINE)
        });
        let counts_as_pure = if inline_val { self.is_idempotent_expr(t) } else { self.is_pure_expr(t) };
        let span = self.tpd.span(t);
        let literal_tpe = constant_type_of(self.u, &value);
        if counts_as_pure {
            return self.tpd.node(T::Literal { value }, span, Some(literal_tpe), None);
        }
        let Some(pre) = pre else { return t };
        let literal = self.tpd.node(T::Literal { value }, span, Some(literal_tpe), None);
        let stats = self.tpd.list(&[pre]);
        self.tpd.node(T::Block { stats, expr: literal }, span, Some(literal_tpe), None)
    }

    /// The prefix of `t` if it is a unary operation of a constant type or a `getClass` call.
    fn dropped_op(&mut self, t: TNodeId) -> Option<TNodeId> {
        match *self.tpd.tree(t) {
            T::Select { qualifier, .. } => Some(qualifier),
            T::Apply { fun, args } if args.is_empty() => {
                let T::TypeApply { fun, .. } = *self.tpd.tree(fun) else { return None };
                let T::Select { qualifier, name } = *self.tpd.tree(fun) else { return None };
                (self.names.text(name.name) == "getClass").then_some(qualifier)
            }
            _ => None,
        }
    }

    /// The constant tree `t` denotes by its type, through type ascriptions, inlined trees without
    /// bindings and blocks without statements (dotty's `ConstantValue`).
    pub fn constant_of(&mut self, t: TNodeId) -> Option<C> {
        match *self.tpd.tree(t) {
            T::Typed { expr, .. } => self.constant_of(expr),
            T::Inlined { bindings: defs, expansion: expr, .. } | T::Block { stats: defs, expr }
                if defs.is_empty() =>
            {
                self.constant_of(expr)
            }
            _ => self.constant_of_tree_type(t),
        }
    }

    fn constant_of_tree_type(&mut self, t: TNodeId) -> Option<C> {
        let tpe = self.tpd.get(t).tpe?;
        self.constant_of_type(tpe)
    }

    /// The constant operand `t` denotes, through inlined trees without bindings and type
    /// ascriptions (dotty's `ConstFold.ConstantTree`).
    pub fn constant_operand(&mut self, t: TNodeId) -> Option<C> {
        match *self.tpd.tree(t) {
            T::Inlined { bindings, expansion, .. } if bindings.is_empty() => self.constant_operand(expansion),
            T::Typed { expr, .. } => self.constant_operand(expr),
            T::Literal { value: C::Null } => Some(C::Null),
            _ => self.constant_of_tree_type(t),
        }
    }

    /// The constant of type `t`, of a constant type or a reference to a term of one.
    pub fn constant_of_type(&mut self, t: TypeId) -> Option<C> {
        match self.u.types().get(t).clone() {
            Type::Str(s) => Some(C::String(s.encode_utf16().collect())),
            Type::Constant(tag, bits) => Some(match constant(tag, bits)? {
                Const::Unit => C::Unit,
                Const::Bool(b) => C::Boolean(b),
                Const::Byte(n) => C::Int(n as i32),
                Const::Short(n) => C::Int(n as i32),
                Const::Char(c) => C::Char(c as u16),
                Const::Int(n) => C::Int(n),
                Const::Long(n) => C::Long(n),
                Const::Float(f) => C::Float(f),
                Const::Double(d) => C::Double(d),
                _ => return None,
            }),
            Type::ByName(r) => self.constant_of_type(r),
            Type::Sym(_, s) if self.u.symbol(s).kind.is_term() => {
                let info = self.u.info(s);
                let constant = matches!(self.u.types().get(info), Type::Constant(..) | Type::Str(_));
                constant.then(|| self.constant_of_type(info))?
            }
            _ => None,
        }
    }

    /// A tree of singleton type `t` at `span`: a literal or a reference to a local term.
    pub fn singleton(&mut self, t: TypeId, span: Span) -> Option<TNodeId> {
        let tpe = self.u.types().get(t).clone();
        match tpe {
            Type::Constant(..) | Type::Str(_) => {
                let value = self.constant_of_type(t)?;
                Some(self.tpd.node(T::Literal { value }, span, Some(t), None))
            }
            Type::Sym(prefix, s) if prefix.is_none_or(|p| self.is_elidable_prefix(p, s)) => {
                let name = self.names.intern(&self.u.symbol(s).name.clone());
                let name = TName { name, is_type: false };
                Some(self.tpd.node(T::Ident { name }, span, Some(t), Some(s)))
            }
            Type::Sym(Some(p), s) if self.u.symbol(s).kind.is_term() => {
                let qualifier = self.singleton(p, Span::at(span.start()))?;
                let name = self.names.intern(&self.u.symbol(s).name.clone());
                let name = TName { name, is_type: false };
                Some(self.tpd.node(T::Select { qualifier, name }, span, Some(t), Some(s)))
            }
            Type::This(_) => self.this_tree(t, span),
            _ => None,
        }
    }

    /// `C.this` of type `t` at the start of `span`, its qualifier the untyped name of the class
    /// (dotty's `This(cls)`).
    pub fn this_tree(&mut self, t: TypeId, span: Span) -> Option<TNodeId> {
        let Type::This(class) = *self.u.types().get(t) else { return None };
        let cls = self.u.class_symbol(class)?;
        let qual = TName { name: self.names.intern(&self.u.symbol(cls).name.clone()), is_type: true };
        let qual = self.tpd.node(T::Ident { name: qual }, span.start_pos(), None, None);
        Some(self.tpd.node(T::This { qual }, span.start_pos(), Some(t), Some(cls)))
    }

    /// Whether `singleton` makes a tree of type `t`: a constant, or a term reference through a
    /// prefix written as an identifier or itself such a reference.
    pub fn is_singleton_path(&mut self, t: TypeId) -> bool {
        match *self.u.types().get(t) {
            Type::Constant(..) | Type::Str(_) | Type::This(_) => true,
            Type::Sym(None, s) => self.u.symbol(s).kind.is_term(),
            Type::Sym(Some(p), s) => {
                self.u.symbol(s).kind.is_term()
                    && (self.is_elidable_prefix(p, s) || self.is_singleton_path(p))
            }
            _ => false,
        }
    }

    /// Whether a reference to `s` with prefix `p` is written as an identifier: `p` is a package, a
    /// static object or the `this` of either, or `s` is a parameter accessor of the `this` of the
    /// class enclosing the call (dotty's `prefixIsElidable`).
    pub fn is_elidable_prefix(&mut self, p: TypeId, s: SymbolId) -> bool {
        match *self.u.types().get(p) {
            Type::Package(_) => true,
            Type::This(c) => self.u.class_symbol(c).is_some_and(|c| {
                let accessor = self.u.symbol(s).flags.has(PARAMsetter)
                    && !self.u.symbol(c).flags.has(TRAIT)
                    && self.enclosing_class == Some(c);
                self.is_static_owner(c) || accessor
            }),
            Type::Sym(_, m) => {
                self.u.is_module(m) && self.u.module_class(m).is_some_and(|c| self.is_static_owner(c))
            }
            _ => false,
        }
    }

    /// Whether `c` is a package or an object class nested only in packages and objects.
    pub fn is_static_owner(&self, c: SymbolId) -> bool {
        let mut o = Some(c);
        while let Some(x) = o {
            let s = self.u.symbol(x);
            if s.kind == Kind::Package {
                return true;
            }
            if !self.u.is_module_class(x) {
                return false;
            }
            o = s.owner;
        }
        false
    }
}
