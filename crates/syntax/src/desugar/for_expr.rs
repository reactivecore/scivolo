//! `for` comprehensions as calls of `map`, `flatMap`, `foreach` and `withFilter` (dotty's
//! `makeFor`).

use super::Desugar;
use crate::ast::tree::GenCheckMode;
use crate::ast::{Attrs, NodeId, TName, Tree};

impl Desugar<'_> {
    /// The expansion of a `for` with `enums` and `body`; an empty tree for erroneous input.
    pub fn make_for(&mut self, map: TName, flat_map: TName, enums: &[NodeId], body: NodeId) -> NodeId {
        let better = self.version.enables_better_fors();
        let Some((&first, rest)) = enums.split_first() else {
            return if better { body } else { self.ast.node(Tree::Empty) };
        };
        let aliases = rest.iter().take_while(|&&e| self.is_gen_alias(e)).count();
        match *self.ast.tree(first) {
            Tree::GenFrom { .. } if rest.is_empty() => {
                let fun = self.rhs_select(first, map);
                let lambda = self.make_lambda(first, body);
                let apply = self.apply(fun, &[lambda]);
                self.mark_trailing_map(apply, first, map, map, body);
                apply
            }
            Tree::GenFrom { .. } if matches!(self.ast.tree(rest[0]), Tree::GenFrom { .. }) => {
                let cont = self.make_for(map, flat_map, rest, body);
                let fun = self.rhs_select(first, flat_map);
                let lambda = self.make_lambda(first, cont);
                self.apply(fun, &[lambda])
            }
            Tree::GenFrom { .. } if aliases > 0 => {
                let (valeqs, suffix) = rest.split_at(aliases);
                let improved = better
                    && suffix.first().is_none_or(|&s| matches!(self.ast.tree(s), Tree::GenFrom { .. }))
                    && !valeqs.iter().any(|&a| self.is_nested_given_pattern(self.gen_pat(a)));
                if improved {
                    let cont = self.make_for(map, flat_map, rest, body);
                    let has_gen = suffix.iter().any(|&s| matches!(self.ast.tree(s), Tree::GenFrom { .. }));
                    let fun = self.rhs_select(first, if has_gen { flat_map } else { map });
                    let lambda = self.make_lambda(first, cont);
                    let apply = self.apply(fun, &[lambda]);
                    if valeqs.len() > 1 {
                        self.ast.add_attrs(apply, Attrs::TUPLING_FOR_MAP);
                    }
                    apply
                } else {
                    self.tupled_aliases(map, flat_map, first, valeqs, suffix, body)
                }
            }
            Tree::GenFrom { pat, check_mode, .. } if !rest.is_empty() => {
                let with_filter = self.term("withFilter");
                let fun = self.rhs_select(first, with_filter);
                let lambda = self.make_lambda(first, rest[0]);
                let filtered = self.apply(fun, &[lambda]);
                let check_mode = match (better, check_mode) {
                    (false, _) => GenCheckMode::Ignore,
                    (true, GenCheckMode::FilterAlways) => GenCheckMode::FilterAlways,
                    (true, _) => GenCheckMode::Filtered,
                };
                let gen1 = self.ast.node(Tree::GenFrom { pat, expr: filtered, check_mode });
                self.make_for(map, flat_map, &[&[gen1][..], &rest[1..]].concat(), body)
            }
            Tree::GenAlias { .. } if better => {
                let (valeqs, suffix) = enums.split_at(aliases + 1);
                let pdefs: Vec<NodeId> = valeqs
                    .iter()
                    .map(|&valeq| {
                        let Tree::GenAlias { pat, expr } = *self.ast.tree(valeq) else {
                            unreachable!("an alias")
                        };
                        let (defpat, _) = self.make_id_pat(pat);
                        self.make_pat_def(valeq, defpat, Some(expr))
                    })
                    .collect();
                let cont = self.make_for(map, flat_map, suffix, body);
                let stats = self.list(&pdefs);
                self.ast.node(Tree::Block { stats, expr: Some(cont) })
            }
            _ => self.ast.node(Tree::Empty),
        }
    }

    /// `for (p <- g; p1 = e1; ...; pN = eN; suffix)` as a generator of the tuples of the pattern
    /// variables, `for ((p, p1, ..., pN) <- for (x @ p <- g) yield { ...; (x, x1, ..., xN) }; suffix)`.
    fn tupled_aliases(
        &mut self,
        map: TName,
        flat_map: TName,
        first: NodeId,
        valeqs: &[NodeId],
        suffix: &[NodeId],
        body: NodeId,
    ) -> NodeId {
        let Tree::GenFrom { pat, expr, check_mode } = *self.ast.tree(first) else {
            unreachable!("a generator")
        };
        let pats: Vec<NodeId> = valeqs.iter().map(|&a| self.gen_pat(a)).collect();
        let (defpat0, id0) = self.make_id_pat(pat);
        let (defpats, ids): (Vec<NodeId>, Vec<NodeId>) = pats.iter().map(|&p| self.make_id_pat(p)).unzip();
        let pdefs: Vec<NodeId> = valeqs
            .iter()
            .zip(&defpats)
            .map(|(&valeq, &defpat)| {
                let Tree::GenAlias { expr, .. } = *self.ast.tree(valeq) else { unreachable!("an alias") };
                self.make_pat_def(valeq, defpat, Some(expr))
            })
            .collect();
        let gen1 = self.ast.node(Tree::GenFrom { pat: defpat0, expr, check_mode });
        let tuple = self.make_tuple(&[&[id0][..], &ids].concat());
        self.ast.add_attrs(tuple, Attrs::FOR_ARTIFACT);
        let stats = self.list(&pdefs);
        let block = self.ast.node(Tree::Block { stats, expr: Some(tuple) });
        let (map1, flat_map1) = (self.term("map"), self.term("flatMap"));
        let rhs1 = self.make_for(map1, flat_map1, &[gen1], block);
        let allpats = self.make_tuple(&[&[pat][..], &pats].concat());
        let vfrom1 =
            self.ast.node(Tree::GenFrom { pat: allpats, expr: rhs1, check_mode: GenCheckMode::Ignore });
        self.make_for(map, flat_map, &[&[vfrom1][..], suffix].concat(), body)
    }

    fn is_gen_alias(&self, t: NodeId) -> bool {
        matches!(self.ast.tree(t), Tree::GenAlias { .. })
    }

    fn gen_pat(&self, t: NodeId) -> NodeId {
        match *self.ast.tree(t) {
            Tree::GenAlias { pat, .. } | Tree::GenFrom { pat, .. } => pat,
            _ => unreachable!("a generator"),
        }
    }

    /// `(ts)` as one tree: the tree itself if one, a `Tuple` otherwise (dotty's `makeTuple`).
    pub fn make_tuple(&mut self, ts: &[NodeId]) -> NodeId {
        match ts {
            [t] if !matches!(self.ast.tree(*t), Tree::NamedArg { .. }) => *t,
            _ => {
                let trees = self.list(ts);
                self.ast.node(Tree::Tuple { trees })
            }
        }
    }
}
