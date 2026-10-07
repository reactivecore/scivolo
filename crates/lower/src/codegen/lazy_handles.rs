//! The `VarHandle` fields dotty's `LazyVals` gives lazy values and Scala Native's accessors of
//! them: a static field `x$lzyN$lzyHandle` of the object or companion object, never set, and a
//! private static method of the class reading it.

use super::Gen;
use super::method::Method;
use crate::symbols::is_module_class;
use nir::sig::{Scope, Sig};
use nir::{Attr, Body, Defn, DefnKind, GlobalId, LexicalScope, Op, PosId, Type, Val};
use tasty::{Kind, SymbolId};

impl Gen<'_, '_> {
    /// The fields of the `VarHandle`s of the lazy values of class `c`, the latest first.
    pub fn gen_lazy_handle_fields(&mut self, c: SymbolId, pos: PosId) {
        let fields: Vec<SymbolId> =
            self.l.lazy_fields.iter().filter(|f| f.class == c).map(|f| f.field).collect();
        for &f in fields.iter().rev() {
            let (name, _) = self.handle(c, f);
            let ty = self.handle_type();
            let rhs = self.val(Val::Zero(ty));
            self.defns.push(Defn { name, attrs: vec![Attr::Final], pos, kind: DefnKind::Var { ty, rhs } });
        }
    }

    /// The static methods of class `c` reading the `VarHandle`s of its lazy values.
    pub fn gen_lazy_handle_accessors(&mut self, c: SymbolId, pos: PosId) {
        let fields: Vec<SymbolId> =
            self.l.lazy_fields.iter().filter(|f| f.class == c).map(|f| f.field).collect();
        for f in fields {
            let (field, holder) = self.handle(c, f);
            let ty = self.handle_type();
            let class = self.type_name(c);
            let id = format!("{}$lzyHandle", self.local_name(f));
            let name = self.pool.member(class, &Sig::Method(id, vec![ty], Scope::PrivateStatic(class)));
            self.m = Method::new(None, 0, pos);
            let entry = self.fresh();
            self.label(entry, Vec::new());
            let obj = self.emit(Op::Module(holder));
            let value = self.emit(Op::Fieldload { ty, obj, name: field });
            self.ret(value);
            let m = std::mem::replace(&mut self.m, Method::new(None, 0, pos));
            let scopes = vec![LexicalScope { id: 0, parent: 0, pos: self.no_pos }];
            let body =
                Body { insts: m.buf.insts, positions: m.buf.positions, local_names: Vec::new(), scopes };
            let fn_ty = self.ty(Type::Function(Vec::new(), ty));
            self.defns.push(Defn {
                name,
                attrs: vec![Attr::InlineHint],
                pos,
                kind: DefnKind::Define { ty: fn_ty, body },
            });
        }
    }

    /// The `VarHandle` field of lazy field `f` of class `class`, and the object holding it.
    fn handle(&mut self, class: SymbolId, f: SymbolId) -> (GlobalId, GlobalId) {
        let holder = match self.handle_holder_class(class) {
            Some(h) => self.type_name(h),
            None => {
                let top = self.type_name(class);
                let name = format!("{}$", self.pool.top_name(top));
                self.pool.top(&name)
            }
        };
        let id = format!("{}$lzyHandle", self.local_name(f));
        (self.pool.member(holder, &Sig::Field(id, Scope::Private(holder))), holder)
    }

    /// The object class holding the `VarHandle`s of class `c`'s lazy values: `c` itself, or its
    /// companion's class if the unit defines one.
    fn handle_holder_class(&mut self, c: SymbolId) -> Option<SymbolId> {
        if is_module_class(self.l.u, c) {
            return Some(c);
        }
        let owner = self.l.u.symbol(c).owner?;
        let name = self.l.u.symbol(c).name.clone();
        let module =
            self.l.u.decl(owner, &name).ok()?.into_iter().find(|&s| self.l.u.symbol(s).kind == Kind::Val)?;
        let class = self.l.u.module_class(module)?;
        self.l.u.symbol(class).tree.is_none().then_some(class)
    }

    fn handle_type(&mut self) -> nir::TypeId {
        let var_handle = self.l.defs.lazy_vals.var_handle;
        self.class_ref(var_handle)
    }
}
