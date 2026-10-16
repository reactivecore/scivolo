//! The NIR body of a bridge: converted arguments, a virtual call, the converted result.

use super::{box_class, is_ref};
use crate::{LoadError, World};
use nir::{Body, Defn, DefnKind, GlobalId, Inst, Local, Next, Op, Param, Type, TypeId, Val, ValId};

/// The instructions of a bridge body and its last local.
struct Emit {
    insts: Vec<Inst>,
    next: u64,
}

impl Emit {
    fn assign(&mut self, op: Op) -> Local {
        self.next += 1;
        let id = Local(self.next);
        self.insts.push(Inst::Let { id, op, unwind: Next::None, scope: 0 });
        id
    }
}

/// Method `name` of mangled types `wanted`, calling `target` of mangled types `tys`.
pub fn bridge_defn(
    world: &mut World,
    name: GlobalId,
    wanted: &[String],
    target: GlobalId,
    tys: &[String],
) -> Result<Defn, LoadError> {
    let owner = world.owner(name);
    let target_defn = world.defn(target)?;
    let DefnKind::Define { ty: target_ty, .. } = target_defn.kind else { unreachable!("a Define") };
    let Type::Function(target_params, target_ret) = world.pool.ty(target_ty).clone() else {
        unreachable!("a method type")
    };
    let this = object_type(world, owner);
    let params: Vec<Param> = std::iter::once(this)
        .chain(wanted[..wanted.len() - 1].iter().map(|t| mangled_type(world, t)))
        .enumerate()
        .map(|(i, ty)| Param { id: Local(i as u64 + 1), ty })
        .collect();
    let mut e = Emit { insts: vec![Inst::Label(Local(0), params.clone())], next: params.len() as u64 };
    let receiver = intern_val(world, Val::Local(params[0].id, this));
    let mut args = vec![receiver];
    for (i, p) in params.iter().enumerate().skip(1) {
        let v = intern_val(world, Val::Local(p.id, p.ty));
        args.push(convert(world, &mut e, v, &wanted[i - 1], &tys[i - 1], target_params[i]));
    }
    let ptr_ty = intern_type(world, Type::Ptr);
    let target_sig = world.pool.member_sig(target).map(str::to_owned).expect("a member");
    let target_sig = nir::StrId(world.pool.strings.intern_str(&target_sig));
    let method = e.assign(Op::Method { obj: receiver, sig: target_sig });
    let ptr = intern_val(world, Val::Local(method, ptr_ty));
    let result = e.assign(Op::Call { ty: target_ty, ptr, args });
    let result = intern_val(world, Val::Local(result, target_ret));
    let (w_ret, t_ret) = (&wanted[wanted.len() - 1], &tys[tys.len() - 1]);
    let w_ret_ty = mangled_type(world, w_ret);
    let out = convert(world, &mut e, result, t_ret, w_ret, w_ret_ty);
    e.insts.push(Inst::Ret(out));
    let pos = target_defn.pos;
    let positions = vec![pos; e.insts.len()];
    let body = Body { positions, insts: e.insts, local_names: Vec::new(), scopes: Vec::new() };
    let ty = intern_type(world, Type::Function(params.iter().map(|p| p.ty).collect(), w_ret_ty));
    Ok(Defn { name, attrs: Vec::new(), pos, kind: DefnKind::Define { ty, body } })
}

/// `v` of mangled type `from` as a value of mangled type `to` and type `to_ty`: boxed,
/// unboxed, cast to a class other than `Object`, or unit.
fn convert(world: &mut World, e: &mut Emit, v: ValId, from: &str, to: &str, to_ty: TypeId) -> ValId {
    let object = object_type_named(world, "java.lang.Object");
    let (op, ty) = match (box_class(from), box_class(to)) {
        _ if to == "u" || (from == "u" && is_ref(to)) => return intern_val(world, Val::Unit),
        _ if from == to && !is_ref(to) => return v,
        _ if is_ref(from) && is_ref(to) && to_ty == object => return v,
        _ if is_ref(from) && is_ref(to) => (Op::As { ty: to_ty, obj: v }, to_ty),
        (Some(b), None) => (Op::Box { ty: object_type_named(world, b), obj: v }, to_ty),
        (None, Some(b)) => (Op::Unbox { ty: object_type_named(world, b), obj: v }, to_ty),
        _ => unreachable!("checked by cost"),
    };
    let id = e.assign(op);
    intern_val(world, Val::Local(id, ty))
}

/// The type of mangled type `t`: its primitive, `unit`, else `java.lang.Object`.
fn mangled_type(world: &mut World, t: &str) -> TypeId {
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
        _ => return object_type_named(world, "java.lang.Object"),
    };
    intern_type(world, ty)
}

fn object_type_named(world: &mut World, name: &str) -> TypeId {
    let g = world.top(name);
    object_type(world, g)
}

fn object_type(world: &mut World, name: GlobalId) -> TypeId {
    intern_type(world, Type::Ref { name, exact: false, nullable: true })
}

fn intern_type(world: &mut World, ty: Type) -> TypeId {
    TypeId(world.pool.types.intern(ty))
}

fn intern_val(world: &mut World, v: Val) -> ValId {
    ValId(world.pool.vals.intern(v))
}
