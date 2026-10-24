//> using scala 3.9.0
object Nt24Bar23 {
  type I23 = Int *: Int *: Int *: Int *: Int *: Int *: Int *: Int *: Int *: Int *: Int *: Int *: Int *: Int *:
    Int *: Int *: Int *: Int *: Int *: Int *: Int *: Int *: Int *: EmptyTuple

  def unapply(x: String): Option[I23] =
    Some((1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, x.length))
}

object Nt24xxlcons {
  def first(s: String): Int = s match {
    case Nt24Bar23((u1, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, u23)) => u1 + u23
    case _ => 0
  }

  def main(args: Array[String]): Unit = {
    val t = (1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23)
    val p: Product = 0 *: t
    println(p.productArity)
    println(p.productElement(23))
    println(first("abc"))
  }
}
