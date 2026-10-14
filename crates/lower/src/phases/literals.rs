//! Literals, wildcards, references and type tests the phases make, most without spans until
//! the trees around them get theirs; the type dotty gives a match.

use crate::Lower;
use crate::definitions::type_ref;
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::{Erased, Type, TypeId};
use typer::tpd::{List, TNodeId, Tree};

impl Lower<'_> {
    pub fn int_literal(&mut self, n: i32) -> TNodeId {
        self.build().literal(Constant::Int(n), Span::NONE)
    }

    pub fn string_literal(&mut self, s: &str) -> TNodeId {
        let tpe = self.u.intern_type(Type::Str(s.to_owned()));
        let value = Constant::String(syntax::names::to_utf16(s).into());
        self.tpd.node(Tree::Literal { value }, Span::NONE, Some(tpe), None)
    }

    pub fn boolean_literal(&mut self, b: bool) -> TNodeId {
        self.build().literal(Constant::Boolean(b), Span::NONE)
    }

    /// `_` of type `tpe`: a pattern, or the initializer of a field without one.
    pub fn wildcard(&mut self, tpe: TypeId) -> TNodeId {
        let name = self.term_name("_");
        self.tpd.node(Tree::Ident { name }, Span::NONE, Some(tpe), None)
    }

    /// A new reference to what identifier `t` refers to.
    pub fn copy_ident(&mut self, t: TNodeId) -> TNodeId {
        let s = self.tpd.sym(t).expect("an identifier's symbol");
        self.ident(s, Span::NONE)
    }

    /// `t.isInstanceOf[tpe]`.
    pub fn is_instance(&mut self, t: TNodeId, tpe: TypeId) -> TNodeId {
        let m = self.defs.is_instance_of;
        let fun = self.select(t, m, Span::NONE);
        let fun = self.positioned(fun);
        let tpt = self.type_tree(tpe, Span::NONE);
        let args = self.tpd.list(&[tpt]);
        let boolean = self.u.class_ref("scala.Boolean");
        let test = self.tpd.node(Tree::TypeApply { fun, args }, Span::NONE, Some(boolean), Some(m));
        self.positioned(test)
    }

    /// The least upper bound of the types of the bodies of `cases`, the type dotty gives a match.
    pub fn cases_lub(&mut self, cases: List) -> TypeId {
        let nothing = self.u.builtins().nothing;
        let mut lub = type_ref(self.u, nothing);
        for c in self.tpd.items(cases).to_vec() {
            let Tree::CaseDef { body, .. } = *self.tpd.tree(c) else { continue };
            let tpe = self.tpd.tpe(body);
            lub = self.u.lub(lub, tpe);
        }
        lub
    }

    /// The zero of erased type `e` at `span`: `false`, `0`, `()` or `null` (dotty's
    /// `defaultValue` after erasure).
    pub fn default_value(&mut self, e: &Erased, span: Span) -> TNodeId {
        let value = match e {
            Erased::Prim('z') => Constant::Boolean(false),
            Erased::Prim('c') => Constant::Char(0),
            Erased::Prim('b') => Constant::Byte(0),
            Erased::Prim('s') => Constant::Short(0),
            Erased::Prim('i') => Constant::Int(0),
            Erased::Prim('j') => Constant::Long(0),
            Erased::Prim('f') => Constant::Float(0.0),
            Erased::Prim('d') => Constant::Double(0.0),
            Erased::Value(_, u) => return self.default_value(u, span),
            Erased::Unit => Constant::Unit,
            _ => Constant::Null,
        };
        self.build().literal(value, span)
    }
}

/// Whether `t` is `_`, a wildcard.
pub fn is_wildcard(l: &Lower, t: TNodeId) -> bool {
    matches!(*l.tpd.tree(t), Tree::Ident { name } if l.names.text(name.name) == "_")
}
