//! `res` values without a value to show, of type `Unit` or `Nothing`: their right-hand sides
//! become statements of the wrapper, their show methods go.

use crate::show_methods::SUFFIX;
use tasty::{Erased, Universe};
use typer::Typed;
use typer::tpd::{TNodeId, Tree};

/// Drops the `res` values of type `Unit` or `Nothing` from wrapper template `template` of
/// `typed`; how many there were.
pub fn drop(u: &mut Universe, typed: &mut Typed, template: TNodeId) -> u32 {
    let Tree::Template { body, .. } = *typed.tpd.tree(template) else { return 0 };
    let stats = typed.tpd.items(body).to_vec();
    let mut dropped: Vec<String> = Vec::new();
    let mut kept = Vec::new();
    for s in stats {
        let sym = typed.tpd.sym(s);
        if let (Tree::ValDef { rhs: Some(rhs), .. }, Some(sym)) = (typed.tpd.tree(s).clone(), sym)
            && is_res(&u.symbol(sym).name)
        {
            let info = u.info(sym);
            if matches!(u.erase(info), Erased::Unit | Erased::Nothing) {
                dropped.push(u.symbol(sym).name.clone());
                u.unenter(sym);
                kept.push(rhs);
                continue;
            }
        }
        kept.push(s);
    }
    let shows: Vec<String> = dropped.iter().map(|n| format!("{n}{SUFFIX}")).collect();
    kept.retain(|&s| match typed.tpd.sym(s) {
        Some(sym)
            if matches!(typed.tpd.tree(s), Tree::DefDef { .. }) && shows.contains(&u.symbol(sym).name) =>
        {
            u.unenter(sym);
            false
        }
        _ => true,
    });
    let kept = typed.tpd.list(&kept);
    if let Tree::Template { body, .. } = &mut typed.tpd.get_mut(template).tree {
        *body = kept;
    }
    dropped.len() as u32
}

fn is_res(name: &str) -> bool {
    name.strip_prefix("res").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The template of the class definition of `module` in `typed`.
pub fn wrapper_template(u: &Universe, typed: &Typed, module: &str) -> Option<TNodeId> {
    let mut todo = vec![typed.root];
    while let Some(t) = todo.pop() {
        match *typed.tpd.tree(t) {
            Tree::PackageDef { stats, .. } => todo.extend(typed.tpd.items(stats)),
            Tree::TypeDef { rhs, .. } if typed.tpd.sym(t).is_some_and(|s| u.symbol(s).name == module) => {
                return Some(rhs);
            }
            _ => {}
        }
    }
    None
}
