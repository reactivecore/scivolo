//> using scala 3.9.0
class P(val x: Int)

object Eq:
  def same(a: String, b: String): Boolean = a == b
  def differ(a: P, b: P): Boolean = a != b
  def isNull(s: String): Boolean = s == null
  def notNull(s: String): Boolean = s != null
  def literal(s: String): Boolean = "a" == s
  def anys(a: Any, b: Any): Boolean = a == b
  def objects(a: AnyRef, b: AnyRef): Boolean = a != b
  def numbers(a: Integer, b: Integer): Boolean = a == b
  def hash(p: P): Int = p.hashCode()
  def hashHash(s: String): Int = s.##
  def show(s: String): String = s.toString()
  def equal(a: String, b: String): Boolean = a.equals(b)

@main def refs(): Unit =
  val p = new P(1)
  println(Eq.same("a", "a"))
  println(Eq.same("a", null))
  println(Eq.differ(p, p))
  println(Eq.differ(p, new P(1)))
  println(Eq.isNull(null))
  println(Eq.notNull("x"))
  println(Eq.literal("a"))
  println(Eq.anys(1, 1))
  println(Eq.anys(1, 1L))
  println(Eq.anys(null, null))
  println(Eq.objects("a", "a"))
  println(Eq.numbers(null, null))
  println(Eq.hash(p) == Eq.hash(p))
  println(Eq.hashHash("ab"))
  println(Eq.show("s"))
  println(Eq.equal("a", "b"))
