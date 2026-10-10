//! The order in which dotty's `LambdaLift` names what it lifts: the proxies of the functions and
//! local classes capturing values directly, then those of the others, each group by position;
//! then the lifted functions and classes in the order they are defined.

use super::lambda_lift::{Function, fresh};
use super::lifted::Method;
use crate::Lower;
use rustc_hash::FxHashSet;
use syntax::source::Offset;
use tasty::{Kind, SymbolId};
use typer::tpd::TNodeId;

/// The proxies of a function, local class or constructor, at the definition's start; whether it
/// uses the values itself.
pub struct Proxies {
    pub at: Offset,
    pub direct: bool,
    pub syms: Vec<SymbolId>,
}

/// A lifted local class to name, at its definition's start, and the class it is in.
pub struct LocalClass {
    pub at: Offset,
    pub class: SymbolId,
    pub outer: SymbolId,
}

/// The proxies and local classes `local_classes` leaves for `lambda_lift` to name, and where the
/// classes were: the values each used at its definition, by block and statement index, and the
/// values passed to its constructor calls.
#[derive(Default)]
pub struct Pending {
    pub proxies: Vec<Proxies>,
    pub classes: Vec<LocalClass>,
    pub definitions: Vec<(TNodeId, usize, Vec<SymbolId>)>,
    pub passed: FxHashSet<TNodeId>,
    next_class: usize,
}

impl Pending {
    /// Names the local classes defined before `at`, all left if `None`.
    pub fn name_classes_before(&mut self, l: &mut Lower, at: Option<Offset>) {
        while let Some(c) = self.classes.get(self.next_class).filter(|c| at.is_none_or(|at| c.at < at)) {
            super::local_lift::name_class(l, c.class, c.outer);
            self.next_class += 1;
        }
    }
}

/// The functions of a class to lift and their methods, the functions in the order their proxies
/// are named and the number of those capturing directly.
pub struct Named<'a> {
    pub class: SymbolId,
    pub functions: &'a [Function],
    pub methods: &'a [Method],
    pub order: &'a [usize],
    pub direct: usize,
}

/// Names the proxies of the functions of `named` and of the local classes of `pending`, then the
/// functions' methods and the local classes, each class's in order, merged by position.
pub fn name_all(l: &mut Lower, named: &[Named], pending: &mut Pending) {
    let proxies: Vec<Vec<Proxies>> = named
        .iter()
        .map(|c| {
            let at = |i: usize| l.tpd.span(c.functions[i].def).start();
            let proxies = c.order.iter().enumerate();
            proxies
                .map(|(k, &i)| Proxies {
                    at: at(i),
                    direct: k < c.direct,
                    syms: c.methods[i].proxies.clone(),
                })
                .collect()
        })
        .collect();
    name_proxies(l, &proxies, pending);
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

/// Names the proxies of the functions of each class of `functions` and of the local classes of
/// `pending`, those used directly first, each class's in order, merged by position, functions'
/// first on a tie.
fn name_proxies(l: &mut Lower, functions: &[Vec<Proxies>], pending: &Pending) {
    for direct in [true, false] {
        let group = |ps: &[Proxies]| -> Vec<(Offset, SymbolId)> {
            let ps = ps.iter().filter(|p| p.direct == direct);
            ps.flat_map(|p| p.syms.iter().map(|&s| (p.at, s))).collect()
        };
        let mut sequences: Vec<_> = functions.iter().map(|ps| group(ps)).collect();
        sequences.push(group(&pending.proxies));
        for s in merged(&sequences) {
            let base = l.u.symbol(s).name.clone();
            let name = fresh(l, &base);
            l.u.rename(s, &name);
        }
    }
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
