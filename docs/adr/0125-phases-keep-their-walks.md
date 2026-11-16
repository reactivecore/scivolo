# ADR 0125 — Phases keep their walks

Status: accepted · Date: 2026-11-16

## Context
Lowering runs about 70 walks over the unit or over each class definition, one or more per
phase. dotty's `MegaPhase` runs the mini-phases of a group in one traversal, each one's
`transform` on a tree after the later ones of the group transformed its subtrees. Our phases
do not follow that contract: many rewrite trees they have not left yet, read trees outside the
one they are at, or finish their work after their walk (`tuple_optimizations`, `tailrec`,
`eval_once`), and several walk nested classes again (`subtrees` of a class definition).
Interleaving two of them changes which trees the second one sees, and with it the output.

On upickle-core (149 classes, 27k trees per unit walk, 26k per round of class walks) a walk
over the unit costs about 6M of 3.8G instructions (0.16 %). Adjacent walks that only collect
and could share one without that proof are few, e.g. `nullable_fields::collect` and the
method list of `tailrec::name_vars`.

## Decision
- Each phase keeps its own walk; no two phases share one.
- The walks are made cheaper instead: a tree's children are appended to the walk's stack in
  place (`Tpd::extend_children` through `Tree::each_slot`), without an intermediate list.

## Consequences
- The phases stay independent: one can be changed, moved or ported from dotty without
  checking the others of a group.
- The traversal costs what one pass over the trees costs times the number of walks; a cost
  that matters comes from what a phase does at a tree, not from the walk.
- A later fusion needs a phase written as dotty's mini-phase (a transform of a tree whose
  subtrees are done) and its own proof that the output stays the same.
