//> using scala 3.9.0
case class P(x: Int, name: String)
case class Mixed(flag: Boolean, c: Char, b: Byte, s: Short, l: Long, f: Float, d: Double)
case class Refs(a: String, b: Option[String])
case object Single
case class Shown(n: Int):
  override def toString: String = "Shown!" + n
final case class Fin(v: Int)

@main def caseclasses(): Unit =
  val p = P(1, "a")
  println(p)
  println(p == P(1, "a"))
  println(p == P(2, "a"))
  println(p.hashCode == P(1, "a").hashCode)
  println(p.copy(5, "c"))
  println(p.productArity)
  println(p.productPrefix)
  println(p.productElement(1))
  println(p.productElementName(0))
  val m = Mixed(true, 'c', 1.toByte, 2.toShort, 3L, 4.5f, 6.5)
  println(m)
  println(m == Mixed(true, 'c', 1.toByte, 2.toShort, 3L, 4.5f, 6.5))
  println(m.hashCode)
  val r = Refs("x", Some("y"))
  println(r)
  println(r.hashCode)
  println(Single)
  println(Single.hashCode)
  println(Single.productArity)
  println(Shown(3))
  println(Fin(2) == Fin(2))
