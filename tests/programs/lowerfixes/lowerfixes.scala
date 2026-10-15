//> using scala 3.9.0
// Lambdas creating inner classes of their object; nested objects reached through package paths;
// array literals passed as arrays of a type parameter.

package pkg {
  object Outer {
    println("init Outer")
    object In {
      println("init In")
      def f = 5
    }
  }
}

object Maker {
  class Item(val v: Int)
  val first = () => new Item(1)
  def make = () => new Item(2)
  def many(n: Int) = List(n).map(k => new Item(k).v + 1)
}

object lowerfixes {
  def refs[T <: AnyRef](xs: Array[T]): Int = xs.length

  def main(args: Array[String]): Unit = {
    val s = Array("a", "b")
    println(refs(Array("r")) + refs(s) + refs(Array(Array(1))))
    println(Maker.first().v + Maker.make().v)
    println(Maker.many(3))
    println(pkg.Outer.In.f)
  }
}
