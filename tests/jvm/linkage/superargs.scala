package superargs

class Holder[A](val value: () => A)

class Top[A <: AnyRef](x: A) extends Holder[A](() => x) {
  def this(xs: Array[A]) = this(xs(0))
}

class Outer[B] {
  class Nested[C](c: C, b: B) extends Holder[(B, C)](() => (b, c))
  def local[D](d: D): Holder[D] = {
    class Local[E](e: E) extends Holder[(D, E)](() => (d, e))
    new Holder(() => new Local("e").value()._1)
  }
}

object Main {
  def main(args: Array[String]): Unit = {
    println(new Top(Array("first")).value())
    val outer = new Outer[Int]
    println(new outer.Nested("c", 1).value())
    println(outer.local("d").value())
  }
}
