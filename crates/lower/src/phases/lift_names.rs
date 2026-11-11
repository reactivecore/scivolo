// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NameOps.scala
//   compiler/src/dotty/tools/dotc/transform/LambdaLift.scala
//! The order in which dotty's `LambdaLift` names what it lifts: the proxies of the functions and
//! local classes in the order of the unit's dependencies; then the lifted functions and classes
//! in the order they are defined.

use super::dependencies::Dependencies;
use super::drop_inlined::naming_order;
use super::lambda_lift::Function;
use super::lifted::Method;
use super::proxy_names::name_proxies;
use crate::symbols::has;
use crate::{Lower, Result};
use names::{name_kinds, nme};
use rustc_hash::FxHashSet;
use syntax::source::Offset;
use tasty::tags::LAZY;
use tasty::{Kind, NameId, SymbolId};
use typer::tpd::TNodeId;

/// The proxy `sym` of a function, local class or constructor `owner` for value `value`.
#[derive(Clone, Copy)]
pub struct Proxy {
    pub owner: SymbolId,
    pub value: SymbolId,
    pub sym: SymbolId,
}

/// A lifted local class to name, at its definition's naming order, and the class it is in.
pub struct LocalClass {
    pub at: (Offset, Offset),
    pub class: SymbolId,
    pub outer: SymbolId,
}

/// A lifted local class calling local methods: its definition, and its fields and constructor
/// parameters standing for the values it uses, each with the value.
pub struct Caller {
    pub td: TNodeId,
    pub class: SymbolId,
    pub fields: Vec<(SymbolId, SymbolId)>,
    pub params: Vec<(SymbolId, SymbolId)>,
}

/// The proxies and local classes `local_classes` leaves for `lambda_lift` to name, and where the
/// classes were: the values each used at its definition, by block and statement index, the
/// values passed to its constructor calls, the classes calling local methods, the unit's
/// dependencies, and the proxies to name last, each with its rank.
#[derive(Default)]
pub struct Pending {
    pub proxies: Vec<Proxy>,
    pub deps: Option<Dependencies>,
    pub classes: Vec<LocalClass>,
    pub definitions: Vec<(TNodeId, usize, Vec<SymbolId>)>,
    pub passed: FxHashSet<TNodeId>,
    pub callers: Vec<Caller>,
    pub late: Vec<((usize, usize), SymbolId)>,
    next_class: usize,
}

impl Pending {
    /// Names the local classes defined before `at`, or at `at` unless objects, all left if `None`.
    pub fn name_classes_before(&mut self, l: &mut Lower, at: Option<(Offset, Offset)>) {
        let before = |l: &Lower, c: &LocalClass| {
            at.is_none_or(|at| c.at < at || c.at == at && !l.u.is_module_class(c.class))
        };
        while let Some(c) = self.classes.get(self.next_class).filter(|c| before(l, c)) {
            super::local_lift::name_class(l, c.class, c.outer);
            self.next_class += 1;
        }
    }
}

/// The functions of a class definition to lift and their methods.
pub struct Named<'a> {
    pub td: TNodeId,
    pub class: SymbolId,
    pub functions: &'a [Function],
    pub methods: &'a [Method],
}

/// Names the proxies of the functions of `named` and of the local classes of `pending`, then the
/// functions' methods and the local classes, each class's in order, merged by position, those of
/// a constructor at the start of its class.
pub fn name_all(l: &mut Lower, named: &[Named], pending: &mut Pending) -> Result<()> {
    let mut proxies: Vec<Proxy> = pending.proxies.clone();
    for n in named {
        for (f, m) in n.functions.iter().zip(n.methods) {
            let pairs = f.captured.iter().zip(&m.proxies);
            proxies.extend(pairs.map(|(&value, &sym)| Proxy { owner: f.sym, value, sym }));
        }
    }
    name_proxies(l, &proxies, pending)?;
    let key = |n: &Named, f: &Function| {
        let at = naming_order(l, f.sym, l.tpd.span(f.def).start());
        (if f.ctor { naming_order(l, n.class, l.tpd.span(n.td).start()) } else { at }, at)
    };
    let functions: Vec<Vec<_>> = named
        .iter()
        .enumerate()
        .map(|(c, n)| {
            let mut keyed: Vec<_> =
                n.functions.iter().enumerate().map(|(i, f)| (key(n, f), (key(n, f).0, c, i))).collect();
            keyed.sort_by_key(|&(_, (_, _, i))| !n.functions[i].ctor);
            keyed
        })
        .collect();
    for (at, c, i) in merged(&functions) {
        let n = &named[c];
        let f = &n.functions[i];
        pending.name_classes_before(l, Some(at));
        let name = if f.is_anonymous(l) {
            fresh(l, &name_kinds::anon_fun(&prefix(l, n.class, n.functions, n.methods, f)))
        } else {
            fresh(l, &l.u.symbol(f.sym).name.clone())
        };
        l.u.rename(n.methods[i].sym, &name);
    }
    pending.name_classes_before(l, None);
    Ok(())
}

/// The prefix of the name of lambda `f` of `functions` of `class`: the name of the method it is
/// in, `$init$$` in the constructor.
pub fn prefix(
    l: &Lower,
    class: SymbolId,
    functions: &[Function],
    methods: &[Method],
    f: &Function,
) -> String {
    match l.u.symbol(f.sym).owner {
        Some(o) if l.u.is_constructor(o) && l.u.symbol(o).owner == Some(class) => {
            name_kinds::unique_prefix(nme::TRAIT_CONSTRUCTOR)
        }
        Some(o) if l.u.symbol(o).kind == Kind::Def || has(l.u, o, LAZY) => {
            match functions.iter().position(|x| x.sym == o) {
                Some(p) => name_kinds::unique_prefix(&l.u.symbol(methods[p].sym).name),
                None => name_kinds::unique_prefix(&l.u.symbol(o).name),
            }
        }
        Some(o) if f.ctor && in_field(l, o, class) => name_kinds::unique_prefix(nme::TRAIT_CONSTRUCTOR),
        _ => String::new(),
    }
}

/// Whether `o` is a member of `class` or a value of its constructor.
fn in_field(l: &Lower, o: SymbolId, class: SymbolId) -> bool {
    let owner = l.u.symbol(o).owner;
    let value = l.u.symbol(o).kind == Kind::Val && !has(l.u, o, LAZY);
    owner == Some(class)
        || value && owner.is_some_and(|c| l.u.is_constructor(c) && l.u.symbol(c).owner == Some(class))
}

/// `base$N`, `N` counting the names made from `base` in the unit (dotty's `freshened`).
pub fn fresh(l: &mut Lower, base: &str) -> String {
    let name = l.fresh.next(l.names, &name_kinds::unique_prefix(base));
    l.names.text(name).to_owned()
}

/// `fresh` of type name `base`, interned.
pub fn fresh_name(l: &mut Lower, base: NameId) -> NameId {
    let base = l.u.name(base).to_owned();
    let name = fresh(l, &base);
    l.u.intern_name(&name)
}

/// The items of `sequences` merged by position, each sequence's in its order, an earlier
/// sequence's first on a tie.
pub fn merged<K: Ord + Copy, T: Copy>(sequences: &[Vec<(K, T)>]) -> Vec<T> {
    let mut next = vec![0; sequences.len()];
    let mut out = Vec::new();
    loop {
        let heads = (0..sequences.len()).filter(|&s| next[s] < sequences[s].len());
        let Some(s) = heads.min_by_key(|&s| sequences[s][next[s]].0) else { return out };
        out.push(sequences[s][next[s]].1);
        next[s] += 1;
    }
}
