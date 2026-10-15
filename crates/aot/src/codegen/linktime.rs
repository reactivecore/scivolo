//! `linktime if` conditions, decided while compiling, with the VM's property values except
//! multithreading, which executables run without (ADR 0028).

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;
use nir::{Attr, Bin, Comp, Cond, DefnKind, GlobalId, Pool, Type, Val, ValId};
use vm::Prop;

impl FnBuilder<'_, '_> {
    pub fn is_linktime_resolved(&mut self, g: GlobalId) -> bool {
        self.reach.world.defn(g).is_ok_and(|d| d.attrs.iter().any(|a| matches!(a, Attr::LinktimeResolved)))
    }

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
        match target.filter(|&g| self.is_linktime_resolved(g)) {
            Some(g) => self.linktime_call(g, args).map(Some),
            None => Ok(None),
        }
    }

    /// A call of `@resolvedAtLinktime` method `g`: the property it reads, else a call of its
    /// definition, which takes the arguments without the module receiver.
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

    /// The value a call of `@resolvedAtLinktime` method `g` returns if `g` reads a link-time
    /// property; `None` for one computed from others.
    fn linktime_value(&mut self, g: GlobalId) -> R<Option<Value>> {
        let Some(prop) = resolved(&self.reach.world.pool, g) else { return Ok(None) };
        Ok(Some(match prop {
            Prop::Bool(b) => self.b.ins().iconst(types::I8, b as i64),
            Prop::Int(i) => self.b.ins().iconst(types::I32, i as u32 as i64),
            Prop::Str(s) => {
                let units: Vec<u16> = s.encode_utf16().collect();
                let data = self.decls.string(self.module, self.reach, &units)?;
                let gv = self.module.declare_data_in_func(data, self.b.func);
                self.b.ins().symbol_value(self.decls.ptr, gv)
            }
        }))
    }
}

const MULTITHREADING: &str = "scala.scalanative.meta.linktimeinfo.isMultithreadingEnabled";

/// The value of link-time property `name` in an executable.
fn property_value(name: &str) -> Option<Prop> {
    match name {
        MULTITHREADING => Some(Prop::Bool(false)),
        _ => vm::property(name),
    }
}

/// The value of the `LinktimeInfo` accessor `M<len><owner>D<len><method>…` names: the target
/// platform for the `is<OS>` ones, else the property it stands for.
fn accessor(mangled: &str) -> Option<Prop> {
    let (owner, sig) = vm::parse_member(mangled)?;
    let method = vm::method_name(sig)?;
    let os = |os: &str| Some(Prop::Bool(std::env::consts::OS == os));
    match method {
        "isWindows" => os("windows"),
        "isLinux" => os("linux"),
        "isMac" => os("macos"),
        "isFreeBSD" => os("freebsd"),
        "isOpenBSD" => os("openbsd"),
        "isNetBSD" => os("netbsd"),
        "isImmix" => Some(Prop::Bool(true)),
        "isBoehm" | "isCommix" | "isNone" | "isExperimental" => Some(Prop::Bool(false)),
        "asanEnabled" | "isFreeBSD32" | "hasDebugInfo" => Some(Prop::Bool(false)),
        _ => property_value(&vm::accessor_property(owner, method)?),
    }
}

/// The value of `@resolvedAtLinktime` method `g`, if it reads a link-time property.
pub fn resolved(pool: &Pool, g: nir::GlobalId) -> Option<Prop> {
    accessor(&pool.mangle(g))
}

pub fn decide(pool: &Pool, cond: &Cond) -> R<bool> {
    match cond {
        Cond::Complex { bin, left, right, .. } => {
            let l = decide(pool, left)?;
            match (bin, l) {
                (Bin::And, false) => Ok(false),
                (Bin::Or, true) => Ok(true),
                _ => decide(pool, right),
            }
        }
        Cond::Simple { property, comp, value, .. } => {
            let name = pool.str(*property);
            let Some(actual) = property_value(name).or_else(|| accessor(name)) else {
                return Err(format!("link-time property {name}"));
            };
            let equal = match (actual, pool.val(*value)) {
                (Prop::Bool(b), Val::True) => b,
                (Prop::Bool(b), Val::False) => !b,
                (Prop::Int(i), Val::Int(j)) => i == *j,
                (Prop::Str(s), Val::String(t)) => s == pool.str(*t),
                (_, v) => return Err(format!("link-time value {v:?} for {name}")),
            };
            match comp {
                Comp::Ieq => Ok(equal),
                Comp::Ine => Ok(!equal),
                c => Err(format!("link-time comparison {c:?}")),
            }
        }
    }
}
