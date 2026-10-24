//> using scala 3.9.0
class Lw24Box[T](private val x: T):
  private def pick[Z <: AnyRef](z: Z, n: Int): Z = if n > 0 then z else z
  private var y: T = x
  inline def picked(s: String): String = pick(s, 1)
  private def first[Z](z: Z): T = x
  inline def got: T = first[String]("z")
  inline def copyFrom[U](c: Lw24Box[U]): U = { c.y = c.x; c.y }
  def set[U](c: Lw24Box[U]): U = { c.y = c.x; c.y }

object Lw24accessors:
  def main(args: Array[String]): Unit =
    val b = Lw24Box(5)
    println(b.picked("s"))
    println(b.got)
    println(b.got + 1)
    println(b.copyFrom(Lw24Box("t")))
    println(b.set(Lw24Box(2.5)))
