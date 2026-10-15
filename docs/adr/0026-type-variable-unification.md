# ADR 0026 — Type variables that bound each other are unified, outside implicit searches

Status: accepted · Date: 2026-10-15

## Context
dotty unifies two type variables once each bounds the other (`ConstraintHandling.addLess`,
`unify`): one is replaced by the other, which keeps the variable of the lower nesting level, and
every bound added to a variable is first cleared of symbols nested deeper than it
(`legalBound`, `LevelAvoidMap`). Our constraint records `?a <: ?b` as `?b` in `?a`'s upper bound
and has no nesting levels. Without unification, singleton lower bounds stay below another
variable and are refused, and large literals compare unions of growing bounds again and again
(tests/pos/i20521).

## Decision
- **Unification:** `?v <: ?w` where `?w`'s upper bound already holds `?v` instantiates one to the
  other, which takes the bounds of both. As in dotty, the one of the lower level is kept, `?w`
  at equal levels.
- **Levels:** a variable's level is the number of symbols defined when it was made. A local
  symbol (owned by a method or value) defined after a variable is deeper than it: a lower bound
  added to the variable has local classes replaced by their parents and local values by their
  types, and a unification that would need such a symbol in the kept variable's bounds is not
  made.
- **Implicit searches:** no variables are unified while an implicit argument is searched. Our
  search neither interpolates a candidate's application, as dotty's `typed` does, nor detects
  divergence; with unification, candidates whose variables dotty instantiates there would get
  other instances (tests/pos/t7983), and self-recursive candidates would no longer fail
  (tests/pos/t3883).

## Consequences
- Searches keep their behaviour from before; lifting the restriction needs candidate
  interpolation and divergence checks first.
- The symbol count is a coarse level: a symbol defined after a variable but outside its scope
  counts as deeper, which can only widen a lower bound or prevent a unification.
