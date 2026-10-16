# ADR 0026 — Type variables that bound each other are unified, in implicit searches only across the outermost one

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
  kept variable, are cleared of deeper symbols only where that is exact. At covariant positions
  a local class without members of its own becomes its parents, a local value its dealiased
  type, a class member of a local value is selected on the value's type, and a younger variable
  becomes a new variable above it at the older level (dotty's `legalVar`); a type member of a
  local value becomes its alias, or its bound where not invariant. Any other deeper symbol,
  class, type or value, prevents the unification, and in a lower bound refuses the unit.
- **Checks:** a block whose result type has a variable whose bounds refer to the block's
  definitions, also through other variables, is refused. This also refuses tests/pos/i8900a3,
  whose tree from dotty leaves the block's class in the type of a definition outside it. A
  check of all trees after zonking is not made for the same reason.
- **Implicit searches:** while an implicit argument is searched, only a variable of a candidate
  of the outermost search is unified, with a variable made before that search (`f[B >: C](using
  Ordering[B])` with `ordered[A]: Ordering[A]`; without it the two bound each other in a cycle
  the satisfiability checks cannot close). Other variables are not unified. Our
  search does not interpolate a candidate's application, as dotty's `typed` does; with
  unification in nested searches, candidates whose variables dotty instantiates there would get
  other instances (tests/pos/t7983), and self-recursive candidates would no longer fail
  (tests/pos/t3883).

## Consequences
- The symbol count is a coarse level in both directions: a variable made after a block's
  definitions, as an overloaded method's after its arguments are typed, does not see them as
  deeper, and a symbol defined after a variable but outside its scope counts as deeper. The
  first case is left to the checks, the second only refuses or widens a lower bound.
- Where dotty approximates further (refinements, local aliases, invariant or contravariant
  positions, a block's open variables), we refuse; so a lambda returning a fresh local class
  inside an invariant type (`xs.map { x => class C; Ref(new C) }`) is refused.
