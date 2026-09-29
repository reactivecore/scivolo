//! Bridge methods Scala Native's compiler leaves out of some lambda classes (ADR 0008): a class
//! without method `m(A…): R` but with exactly one `m` of the same arity whose parameter and
//! result types differ only by boxing gets `m(A…): R`, which converts and calls it.

use crate::Vm;
use crate::class::ClassId;
use crate::interp::Flow;
use nir::{
    Body, Defn, DefnKind, Global, GlobalId, Inst, Local, Next, Op, Param, StrId, Type, TypeId, Val, ValId,
};

/// The box class of each primitive mangled type.
const BOXES: [(&str, &str); 8] = [
    ("z", "java.lang.Boolean"),
    ("c", "java.lang.Character"),
    ("b", "java.lang.Byte"),
    ("s", "java.lang.Short"),
    ("i", "java.lang.Integer"),
    ("j", "java.lang.Long"),
    ("f", "java.lang.Float"),
    ("d", "java.lang.Double"),
];

fn is_ref(ty: &str) -> bool {
    ty.starts_with(['L', 'A'])
}

/// Whether a value of mangled type `from` can be passed where `to` is expected by boxing,
/// unboxing or as it is.
fn convertible(from: &str, to: &str) -> bool {
    from == to
        || (is_ref(from) && is_ref(to))
        || (is_ref(to) && (from == "u" || BOXES.iter().any(|(p, _)| *p == from)))
        || (is_ref(from) && BOXES.iter().any(|(p, _)| *p == to))
        || to == "u"
}

impl Vm {
    /// The bridge for method `sig` of class `cls`, defined on first use, if `cls` has exactly one
    /// method it can bridge to.
    pub fn bridge(&mut self, cls: ClassId, sig: StrId) -> Flow<Option<GlobalId>> {
        let Some((name, wanted)) = nir::demangle::method(self.world.pool.str(sig)) else { return Ok(None) };
        let mut found = Vec::new();
        for id in self.lookup_order(cls) {
            let owner = self.class(id).name;
            for m in self.world.members(owner)?.to_vec() {
                let Some((n, tys)) = self.world.pool.member_sig(m).and_then(nir::demangle::method) else {
                    continue;
                };
                let compatible = n == name
                    && tys.len() == wanted.len()
                    && tys.iter().zip(&wanted).enumerate().all(|(i, (t, w))| match i + 1 == tys.len() {
                        true => convertible(t, w),
                        false => convertible(w, t),
                    });
                if compatible && matches!(self.world.defn(m)?.kind, DefnKind::Define { .. }) {
                    found.push((m, tys));
                }
            }
            if !found.is_empty() {
                break;
            }
        }
        let [(target, tys)] = &found[..] else { return Ok(None) };
        let defn = self.bridge_defn(self.class(cls).name, sig, &wanted, *target, tys)?;
        let g = defn.name;
        self.world.define(defn);
        Ok(Some(g))
    }

    /// `owner`'s method `sig` of mangled types `wanted`, calling `target` of types `tys`.
    fn bridge_defn(
        &mut self,
        owner: GlobalId,
        sig: StrId,
        wanted: &[String],
        target: GlobalId,
        tys: &[String],
    ) -> Flow<Defn> {
        let pos = self.world.defn(target)?.pos;
        let this = self.object_type(owner);
        let (w_ret, w_params) = wanted.split_last().expect("a result");
        let (t_ret, t_params) = tys.split_last().expect("a result");
        let mut fresh = (1..).map(Local);
        let params: Vec<Param> = std::iter::once(this)
            .chain(w_params.iter().map(|t| self.mangled_type(t)))
            .map(|ty| Param { id: fresh.next().expect("unbounded"), ty })
            .collect();
        let mut insts = vec![Inst::Label(Local(0), params.clone())];
        let mut args = vec![self.local(&params[0])];
        for ((w, t), p) in w_params.iter().zip(t_params).zip(&params[1..]) {
            let v = self.local(p);
            args.push(self.convert(v, w, t, &mut fresh, &mut insts));
        }
        let fn_ty = |vm: &mut Vm, params: &[String], ret: &str| {
            let tys = std::iter::once(this).chain(params.iter().map(|t| vm.mangled_type(t))).collect();
            let ret = vm.mangled_type(ret);
            vm.intern_type(Type::Function(tys, ret))
        };
        let target_ty = fn_ty(self, t_params, t_ret);
        let result = fresh.next().expect("unbounded");
        let ptr = self.intern_val(Val::Global(target, target_ty));
        insts.push(Inst::Let {
            id: result,
            op: Op::Call { ty: target_ty, ptr, args },
            unwind: Next::None,
            scope: 0,
        });
        let ret_ty = self.mangled_type(t_ret);
        let v = self.intern_val(Val::Local(result, ret_ty));
        let out = self.convert(v, t_ret, w_ret, &mut fresh, &mut insts);
        insts.push(Inst::Ret(out));
        let body =
            Body { positions: vec![pos; insts.len()], insts, local_names: Vec::new(), scopes: Vec::new() };
        let name = GlobalId(self.world.pool.globals.intern(Global::Member(owner, sig)));
        let ty = fn_ty(self, w_params, w_ret);
        Ok(Defn { name, attrs: Vec::new(), pos, kind: DefnKind::Define { ty, body } })
    }

    /// `v` of mangled type `from` as a value of `to`, adding the instructions that box or unbox it.
    fn convert(
        &mut self,
        v: ValId,
        from: &str,
        to: &str,
        fresh: &mut impl Iterator<Item = Local>,
        insts: &mut Vec<Inst>,
    ) -> ValId {
        let boxed = |p: &str| BOXES.iter().find(|(q, _)| *q == p).map(|(_, b)| *b);
        let (op, ty) = match (boxed(from), boxed(to)) {
            _ if from == to || (is_ref(from) && is_ref(to)) => return v,
            _ if to == "u" || (from == "u" && is_ref(to)) => return self.intern_val(Val::Unit),
            (Some(b), None) => {
                let ty = self.object_type_named(b);
                (Op::Box { ty, obj: v }, ty)
            }
            (None, Some(b)) => {
                let ty = self.object_type_named(b);
                (Op::Unbox { ty, obj: v }, self.mangled_type(to))
            }
            _ => unreachable!("checked convertible"),
        };
        let id = fresh.next().expect("unbounded");
        insts.push(Inst::Let { id, op, unwind: Next::None, scope: 0 });
        self.intern_val(Val::Local(id, ty))
    }

    /// The type of mangled type `t`: its primitive, `unit`, else `java.lang.Object`.
    fn mangled_type(&mut self, t: &str) -> TypeId {
        let ty = match t {
            "z" => Type::Bool,
            "c" => Type::Char,
            "b" => Type::Byte,
            "s" => Type::Short,
            "i" => Type::Int,
            "j" => Type::Long,
            "f" => Type::Float,
            "d" => Type::Double,
            "u" => Type::Unit,
            _ => return self.object_type_named("java.lang.Object"),
        };
        self.intern_type(ty)
    }

    fn object_type_named(&mut self, name: &str) -> TypeId {
        let g = self.world.top(name);
        self.object_type(g)
    }

    fn object_type(&mut self, name: GlobalId) -> TypeId {
        self.intern_type(Type::Ref { name, exact: false, nullable: true })
    }

    fn local(&mut self, p: &Param) -> ValId {
        self.intern_val(Val::Local(p.id, p.ty))
    }

    fn intern_type(&mut self, ty: Type) -> TypeId {
        TypeId(self.world.pool.types.intern(ty))
    }

    fn intern_val(&mut self, v: Val) -> ValId {
        ValId(self.world.pool.vals.intern(v))
    }
}
