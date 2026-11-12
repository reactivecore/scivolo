trait Show[A]:
  def show(a: A): String

given Show[String] with
  def show(a: String): String = a

object MatchAliasResults:
  def mk[T <: Tuple](t: T): Tuple.Head[T] = ???
  def id[A](a: A): A = a
  def f[T <: Tuple]: Option[Tuple.Head[T]] = None
  def show[A](a: A)(using s: Show[A]): String = s.show(a)

  def applied = id(mk(("a", 1)))
  def typeApplied = f[(Boolean, Int)]
  def implicitArg = show(mk(("a", 1)))
