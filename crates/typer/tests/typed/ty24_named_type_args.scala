import language.experimental.namedTypeArguments

final case class Ty24Entry[K, V](value: V)

object Ty24NamedTypeArgs:
  def pair[A, B](a: A, b: B): (A, B) = (a, b)
  val e = Ty24Entry[K = "name"]("foo")
  val p = pair[B = Long](1, 2L)
