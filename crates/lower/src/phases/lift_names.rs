//! The order in which dotty's `LambdaLift` names what it lifts: the proxies of the functions and
//! local classes in the order of the unit's dependencies; then the lifted functions and classes
//! in the order they are defined.

use super::dependencies::Dependencies;
use super::lambda_lift::{Function, fresh};
use super::lifted::Method;
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use syntax::ast::Span;
use syntax::source::Offset;
use tasty::{Kind, SymbolId};
use typer::tpd::TNodeId;

/// The proxy `sym` of a function, local class or constructor `owner` for value `value`.
#[derive(Clone, Copy)]
pub struct Proxy {
    pub owner: SymbolId,
    pub value: SymbolId,
    pub sym: SymbolId,
}

/// A lifted local class to name, at its definition's start, and the class it is in.
pub struct LocalClass {
    pub at: Offset,
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
/// values passed to its constructor calls, the classes calling local methods, and the unit's
/// dependencies.
#[derive(Default)]
pub struct Pending {
    pub proxies: Vec<Proxy>,
    pub deps: Option<Dependencies>,
    pub classes: Vec<LocalClass>,
    pub definitions: Vec<(TNodeId, usize, Vec<SymbolId>)>,
    pub passed: FxHashSet<TNodeId>,
    pub callers: Vec<Caller>,
    next_class: usize,
}

impl Pending {
    /// Names the local classes defined before `at`, or at `at` unless objects, all left if `None`.
    pub fn name_classes_before(&mut self, l: &mut Lower, at: Option<Offset>) {
        let before = |l: &Lower, c: &LocalClass| {
            at.is_none_or(|at| c.at < at || c.at == at && !l.u.is_module_class(c.class))
        };
        while let Some(c) = self.classes.get(self.next_class).filter(|c| before(l, c)) {
            super::local_lift::name_class(l, c.class, c.outer);
            self.next_class += 1;
        }
    }
}

/// The functions of a class to lift and their methods.
pub struct Named<'a> {
    pub class: SymbolId,
    pub functions: &'a [Function],
    pub methods: &'a [Method],
}

/// Names the proxies of the functions of `named` and of the local classes of `pending`, then the
/// functions' methods and the local classes, each class's in order, merged by position.
pub fn name_all(l: &mut Lower, named: &[Named], pending: &mut Pending) -> Result<()> {
    let mut proxies: Vec<Proxy> = pending.proxies.clone();
    for n in named {
        for (f, m) in n.functions.iter().zip(n.methods) {
            let pairs = f.captured.iter().zip(&m.proxies);
            proxies.extend(pairs.map(|(&value, &sym)| Proxy { owner: f.sym, value, sym }));
        }
    }
    name_proxies(l, &proxies, pending)?;
    let functions: Vec<Vec<(Offset, (usize, usize))>> = named
        .iter()
        .enumerate()
        .map(|(c, n)| {
            n.functions.iter().enumerate().map(|(i, f)| (l.tpd.span(f.def).start(), (c, i))).collect()
        })
        .collect();
    for (c, i) in merged(&functions) {
        let n = &named[c];
        pending.name_classes_before(l, Some(l.tpd.span(n.functions[i].def).start()));
        let f = &n.functions[i];
        let name = match f.closure {
            Some(_) => fresh(l, &format!("{}$anonfun", prefix(l, n.class, n.functions, n.methods, f))),
            None => fresh(l, &l.u.symbol(f.sym).name.clone()),
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
        Some(o) if l.u.symbol(o).kind == Kind::Def => match functions.iter().position(|x| x.sym == o) {
            Some(p) => format!("{}$", l.u.symbol(methods[p].sym).name),
            None => format!("{}$", l.u.symbol(o).name),
        },
        Some(o) if f.ctor && l.u.symbol(o).owner == Some(class) => "$init$$".to_owned(),
        _ => String::new(),
    }
}

/// Names `proxies` in the order of the dependencies of `pending`: by owner, then by value (dotty's
/// `generateProxies`).
fn name_proxies(l: &mut Lower, proxies: &[Proxy], pending: &Pending) -> Result<()> {
    let mut ranked = Vec::new();
    for p in proxies {
        let Some(rank) = pending.deps.as_ref().and_then(|d| d.rank(p.owner, p.value)) else {
            return unsupported(Span::NONE, "lifting a value the dependencies of the unit do not record");
        };
        ranked.push((rank, p.sym));
    }
    ranked.sort_by_key(|&(rank, _)| rank);
    for (_, s) in ranked {
        let base = l.u.symbol(s).name.clone();
        let name = fresh(l, &base);
        l.u.rename(s, &name);
    }
    Ok(())
}

/// The items of `sequences` merged by position, each sequence's in its order, an earlier
/// sequence's first on a tie.
pub fn merged<T: Copy>(sequences: &[Vec<(Offset, T)>]) -> Vec<T> {
    let mut next = vec![0; sequences.len()];
    let mut out = Vec::new();
    loop {
        let heads = (0..sequences.len()).filter(|&s| next[s] < sequences[s].len());
        let Some(s) = heads.min_by_key(|&s| sequences[s][next[s]].0) else { return out };
        out.push(sequences[s][next[s]].1);
        next[s] += 1;
    }
}
