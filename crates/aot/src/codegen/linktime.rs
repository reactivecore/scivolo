//! `linktime if` conditions and `@resolvedAtLinktime` methods, decided while compiling with the
//! VM's property values except multithreading, which executables run without (ADR 0028).

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;
use nir::{Attr, Bin, Comp, Cond, DefnKind, GlobalId, Inst, Next, Op, StrId, Type, Val, ValId};
use std::collections::HashMap;
use vm::{Prop, World};

impl FnBuilder<'_, '_> {
    /// The value of a call of `ptr` with `args` that is no plain call: a pair accessor, an
    /// intrinsic with an instruction, or a `@resolvedAtLinktime` method.
    pub fn special_call(&mut self, ptr: ValId, args: &[ValId]) -> R<Option<Value>> {
        if let Some(v) = self.overflow_accessor(ptr, args)? {
            return Ok(Some(v));
        }
        if let Some(v) = self.intrinsic(ptr, args)? {
            return Ok(Some(v));
        }
        let target = match *self.reach.world.pool.val(ptr) {
            Val::Global(g, _) => Some(g),
            Val::Local(l, _) => self.linktime_methods.get(&l.0).copied(),
            _ => None,
        };
        match target.filter(|&g| is_linktime(self.reach.world, g)) {
            Some(g) => self.linktime_call(g, args).map(Some),
            None => Ok(None),
        }
    }

    /// A call of `@resolvedAtLinktime` method `g`: the value its body evaluates to, else a call
    /// of its definition, which takes the arguments without the module receiver.
    fn linktime_call(&mut self, g: GlobalId, args: &[ValId]) -> R<Value> {
        if let Some(v) = self.linktime_value(g)? {
            return Ok(v);
        }
        let defn = self.reach.world.defn(g).map_err(|e| e.to_string())?;
        let DefnKind::Define { ty, .. } = defn.kind else {
            return Err("a link-time method without body".into());
        };
        let Type::Function(params, _) = self.reach.world.pool.ty(ty).clone() else {
            return Err("a link-time method of a non-function type".into());
        };
        let receiver = args.len() - params.len();
        let mut values = Vec::with_capacity(params.len());
        for (&a, p) in args[receiver..].iter().zip(params) {
            values.push(self.arg_of(a, p)?);
        }
        let f = self.decls.func(self.module, self.reach, g)?;
        let fref = self.module.declare_func_in_func(f, self.b.func);
        let call = self.b.ins().call(fref, &values);
        Ok(self.b.inst_results(call).first().copied().unwrap_or_else(|| self.b.ins().iconst(types::I8, 0)))
    }

    /// The constant a call of `@resolvedAtLinktime` method `g` returns, if its body evaluates to
    /// a boolean, an `Int` or a string.
    fn linktime_value(&mut self, g: GlobalId) -> R<Option<Value>> {
        Ok(Some(match evaluate(self.reach.world, g)? {
            Some(Val::True) => self.b.ins().iconst(types::I8, 1),
            Some(Val::False) => self.b.ins().iconst(types::I8, 0),
            Some(Val::Int(i)) => self.b.ins().iconst(types::I32, i as u32 as i64),
            Some(Val::String(s)) => {
                let units: Vec<u16> = self.reach.world.pool.str(s).encode_utf16().collect();
                let data = self.decls.string(self.module, self.reach, &units)?;
                let gv = self.module.declare_data_in_func(data, self.b.func);
                self.b.ins().symbol_value(self.decls.ptr, gv)
            }
            _ => return Ok(None),
        }))
    }
}

const MULTITHREADING: &str = "scala.scalanative.meta.linktimeinfo.isMultithreadingEnabled";

pub fn is_linktime(world: &mut World, g: GlobalId) -> bool {
    world.defn(g).is_ok_and(|d| d.attrs.iter().any(|a| matches!(a, Attr::LinktimeResolved)))
}

/// The value of link-time property `name` in an executable: a configured one, else what the
/// `@resolvedAtLinktime` method of that mangled name evaluates to.
fn property(world: &mut World, name: &str) -> R<Val> {
    let configured = match name {
        MULTITHREADING => Some(Prop::Bool(false)),
        _ => vm::property(name),
    };
    let unknown = || format!("link-time property {name}");
    let Some(prop) = configured else {
        let (top, sig) = vm::parse_member(name).ok_or_else(unknown)?;
        let owner = world.top(top);
        let g = world.member(owner, sig);
        return evaluate(world, g)?.ok_or_else(unknown);
    };
    Ok(match prop {
        Prop::Bool(true) => Val::True,
        Prop::Bool(false) => Val::False,
        Prop::Int(i) => Val::Int(i),
        Prop::Str(s) => Val::String(StrId(world.pool.strings.intern_str(s))),
    })
}

/// The value `@resolvedAtLinktime` method `g` returns if its body reads properties and branches
/// on them only: property reads, `linktime if`s and jumps that lead to a `ret`.
fn evaluate(world: &mut World, g: GlobalId) -> R<Option<Val>> {
    if !is_linktime(world, g) {
        return Ok(None);
    }
    let defn = world.defn(g).map_err(|e| e.to_string())?;
    let DefnKind::Define { body, .. } = &defn.kind else { return Ok(None) };
    let labels: HashMap<u64, usize> = (body.insts.iter().enumerate())
        .filter_map(|(at, inst)| match inst {
            Inst::Label(l, _) => Some((l.0, at)),
            _ => None,
        })
        .collect();
    let mut locals: HashMap<u64, Val> = HashMap::new();
    let mut at = 1;
    for _ in 0..body.insts.len() {
        let next = match body.insts.get(at) {
            Some(Inst::Ret(v)) => return Ok(value(world, &locals, *v)),
            Some(Inst::Jump(next)) => next,
            Some(Inst::LinktimeIf(cond, then, els)) => {
                if decide(world, cond)? {
                    then
                } else {
                    els
                }
            }
            Some(Inst::Let { id, op: Op::Call { ptr, args, .. }, .. }) => {
                let Some(name) = resolve_property(world, *ptr, args) else { return Ok(None) };
                let v = property(world, &name)?;
                locals.insert(id.0, v);
                at += 1;
                continue;
            }
            _ => return Ok(None),
        };
        let Next::Label(l, args) = next else { return Ok(None) };
        let Some(&target) = labels.get(&l.0) else { return Ok(None) };
        let Inst::Label(_, params) = &body.insts[target] else { return Ok(None) };
        for (p, &a) in params.iter().zip(args) {
            let Some(v) = value(world, &locals, a) else { return Ok(None) };
            locals.insert(p.id.0, v);
        }
        at = target + 1;
    }
    Ok(None)
}

/// `v` with locals replaced by their values; `None` for a local without one.
fn value(world: &World, locals: &HashMap<u64, Val>, v: ValId) -> Option<Val> {
    match world.pool.val(v) {
        Val::Local(l, _) => locals.get(&l.0).cloned(),
        v => Some(v.clone()),
    }
}

/// The property a call of `ptr` with `args` reads, if it is `linktime.resolveProperty`.
fn resolve_property(world: &World, ptr: ValId, args: &[ValId]) -> Option<String> {
    let Val::Global(f, _) = *world.pool.val(ptr) else { return None };
    let sig = world.pool.member_sig(f)?;
    if world.pool.top_name(f) != "scala.scalanative.linktime" || vm::method_name(sig)? != "resolveProperty" {
        return None;
    }
    let [arg] = args else { return None };
    let Val::String(name) = *world.pool.val(*arg) else { return None };
    Some(world.pool.str(name).to_owned())
}

pub fn decide(world: &mut World, cond: &Cond) -> R<bool> {
    match cond {
        Cond::Complex { bin, left, right, .. } => {
            let l = decide(world, left)?;
            match (bin, l) {
                (Bin::And, false) => Ok(false),
                (Bin::Or, true) => Ok(true),
                _ => decide(world, right),
            }
        }
        Cond::Simple { property, comp, value, .. } => {
            let name = world.pool.str(*property).to_owned();
            let actual = self::property(world, &name)?;
            let expected = world.pool.val(*value);
            let kind = |v: &Val| std::mem::discriminant(if *v == Val::True { &Val::False } else { v });
            if kind(&actual) != kind(expected) {
                return Err(format!("link-time value {expected:?} for {name}"));
            }
            let equal = actual == *expected;
            match comp {
                Comp::Ieq => Ok(equal),
                Comp::Ine => Ok(!equal),
                c => Err(format!("link-time comparison {c:?}")),
            }
        }
    }
}
