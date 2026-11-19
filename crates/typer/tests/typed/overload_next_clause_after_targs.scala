trait TT
object CC extends TT

extension (x: TT)
  def ff[X](): Int = 1
  def ff[X](s: String): Int = s.length
  def ff[X](n: Int): Int = n

object Test:
  def g(x: TT)[X](n: Int): Int = n
  def g(x: TT)[X](s: String): String = s

  val yy = CC.ff[Int]() + CC.ff[String]("abc") + CC.ff[Int](22)
  val a = g(CC)[Int](1)
  val b = g(CC)[Int]("b")
