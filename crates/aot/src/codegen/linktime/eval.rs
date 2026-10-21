// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   tools/src/main/scala/scala/scalanative/linker/LinktimeValueResolver.scala
//! `linktime if` conditions and `@resolvedAtLinktime` methods, evaluated while compiling with
//! the VM's property values except multithreading, which executables run without (ADR 0028).

use crate::R;
use nir::{Attr, Bin, Comp, Cond, DefnKind, GlobalId, Inst, Next, Op, StrId, Val, ValId};
use std::collections::HashMap;
use vm::{Prop, World};

const MULTITHREADING: &str = "scala.scalanative.meta.linktimeinfo.isMultithreadingEnabled";

/// The values of the `@resolvedAtLinktime` methods evaluated so far, `None` for one whose body
/// is no evaluation, and the methods being evaluated.
#[derive(Default)]
pub struct Linktime {
    values: HashMap<GlobalId, Option<Val>>,
    stack: Vec<GlobalId>,
}

pub fn is_linktime(world: &mut World, g: GlobalId) -> bool {
    world.defn(g).is_ok_and(|d| d.attrs.iter().any(|a| matches!(a, Attr::LinktimeResolved)))
}

impl Linktime {
    pub fn decide(&mut self, world: &mut World, cond: &Cond) -> R<bool> {
        match cond {
            Cond::Complex { bin, left, right, .. } => {
                let l = self.decide(world, left)?;
                match (bin, l) {
                    (Bin::And, false) => Ok(false),
                    (Bin::Or, true) => Ok(true),
                    _ => self.decide(world, right),
                }
            }
            Cond::Simple { property: p, comp, value, .. } => {
                let name = world.pool.str(*p).to_owned();
                let actual = self.property(world, &name)?;
                let expected = world.pool.val(*value);
                if !same_kind(&actual, expected) {
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

    /// The value of link-time property `name` in an executable: a configured one, else what the
    /// `@resolvedAtLinktime` method of that mangled name evaluates to.
    fn property(&mut self, world: &mut World, name: &str) -> R<Val> {
        let configured = match name {
            MULTITHREADING => Some(Prop::Bool(false)),
            _ => vm::property(name),
        };
        let Some(prop) = configured else {
            let undefined = || format!("link-time property {name} is not defined");
            let (top, sig) = vm::parse_member(name).ok_or_else(undefined)?;
            let owner = world.top(top);
            let g = world.member(owner, sig);
            if !is_linktime(world, g) {
                return Err(undefined());
            }
            let unsupported =
                || format!("does not support evaluating link-time method {name} at compile time");
            return self.evaluate(world, g)?.ok_or_else(unsupported);
        };
        Ok(match prop {
            Prop::Bool(true) => Val::True,
            Prop::Bool(false) => Val::False,
            Prop::Int(i) => Val::Int(i),
            Prop::Str(s) => Val::String(StrId(world.pool.strings.intern_str(s))),
        })
    }

    /// The value `@resolvedAtLinktime` method `g` returns, once per method; `None` if its body
    /// does more than read properties and branch on them.
    pub fn evaluate(&mut self, world: &mut World, g: GlobalId) -> R<Option<Val>> {
        if let Some(v) = self.values.get(&g) {
            return Ok(v.clone());
        }
        if !is_linktime(world, g) {
            return Ok(None);
        }
        if self.stack.contains(&g) {
            return Err(format!("link-time method {} depends on itself", world.pool.mangle(g)));
        }
        self.stack.push(g);
        let v = self.interpret(world, g);
        self.stack.pop();
        let v = v?;
        self.values.insert(g, v.clone());
        Ok(v)
    }

    /// The value the body of `g` returns if it holds property reads, `linktime if`s and jumps
    /// that lead to a `ret` only.
    fn interpret(&mut self, world: &mut World, g: GlobalId) -> R<Option<Val>> {
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
                    if self.decide(world, cond)? {
                        then
                    } else {
                        els
                    }
                }
                Some(Inst::Let { id, op: Op::Call { ptr, args, .. }, .. }) => {
                    let Some(name) = resolve_property(world, *ptr, args) else { return Ok(None) };
                    let v = self.property(world, &name)?;
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
}

fn same_kind(a: &Val, b: &Val) -> bool {
    matches!(
        (a, b),
        (Val::True | Val::False, Val::True | Val::False)
            | (Val::Int(_), Val::Int(_))
            | (Val::String(_), Val::String(_))
    )
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
