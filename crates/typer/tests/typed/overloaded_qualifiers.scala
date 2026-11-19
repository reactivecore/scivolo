object O {
  def f: Int = 5
  def f(x: Int): Int = x + 1
}

object Ext {
  extension (i: Int) def apply(j: Int): Int = i * j
  extension (i: Int) def andThen(g: Int => Int): Int = g(i) * 1000
}

object Use {
  import Ext.*
  val a = O.f.apply(3)
  val b = O.f.andThen(_ + 1)(3)
  val c = O.f.toString
}
