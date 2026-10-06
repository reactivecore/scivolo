//! Literals, wildcards, references and type tests the phases make, without spans until the
//! trees around them get theirs; the type dotty gives a match.

use crate::Lower;
use crate::definitions::type_ref;
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::{Type, TypeId};
use typer::tpd::{List, TNodeId, Tree};

impl Lower<'_> {
    pub fn int_literal(&mut self, n: i32) -> TNodeId {
        let tpe = self.u.intern_type(Type::Constant(tasty::tags::INTconst, i64::from(n)));
        self.tpd.node(Tree::Literal { value: Constant::Int(n) }, Span::NONE, Some(tpe), None)
    }

    pub fn string_literal(&mut self, s: &str) -> TNodeId {
        let tpe = self.u.intern_type(Type::Str(s.to_owned()));
        let value = Constant::String(syntax::names::to_utf16(s).into());
        self.tpd.node(Tree::Literal { value }, Span::NONE, Some(tpe), None)
    }

    pub fn boolean_literal(&mut self, b: bool) -> TNodeId {
        let tag = if b { tasty::tags::TRUEconst } else { tasty::tags::FALSEconst };
        let tpe = self.u.intern_type(Type::Constant(tag, 0));
        self.tpd.node(Tree::Literal { value: Constant::Boolean(b) }, Span::NONE, Some(tpe), None)
    }

    /// `_` of type `tpe`, a pattern.
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
        let m = self.defs.patmat.is_instance_of;
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
}
