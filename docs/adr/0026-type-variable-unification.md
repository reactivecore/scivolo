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
- **Levels:** a variable's level is the number of symbols defined when it was made; a symbol
  owned by a method or value, or nested in one, defined after the variable counts as deeper.
- **Avoidance:** a lower bound added to a variable, and the bounds a unification moves to the
  kept variable, are cleared of deeper symbols only where that is exact: a local class without
  members of its own at a covariant position becomes its parents, a local value at a covariant
  position its type, a younger variable at a covariant position a new variable above it at the
  older level (dotty's `legalVar`). Any other deeper symbol prevents the unification, and in a
  lower bound refuses the unit if it is a class or type (a value's singleton type stays, as
  before unification).
- **Checks:** a block whose result type has a variable whose bounds refer to the block's classes
  or types, also through other variables, is refused. A check of all trees after zonking is not
  made: dotty itself leaves a block's class in the type of a definition outside it
  (tests/pos/i8900a3).
- **Implicit searches:** no variables are unified while an implicit argument is searched. Our
  search neither interpolates a candidate's application, as dotty's `typed` does, nor detects
  divergence; with unification, candidates whose variables dotty instantiates there would get
  other instances (tests/pos/t7983), and self-recursive candidates would no longer fail
  (tests/pos/t3883).

## Consequences
- Searches keep their behaviour from before; lifting the restriction needs candidate
  interpolation and divergence checks first.
- The symbol count is a coarse level in both directions: a variable made after a block's
  definitions, as an overloaded method's after its arguments are typed, does not see them as
  deeper, and a symbol defined after a variable but outside its scope counts as deeper. The
  first case is left to the checks, the second only refuses or widens a lower bound.
- Where dotty approximates by more than a class's parents (refinements, aliases, invariant
  positions, fresh variables of `legalVar`), we refuse.
