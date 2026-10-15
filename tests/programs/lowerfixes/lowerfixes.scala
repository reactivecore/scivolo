//> using scala 3.9.0
// Lambdas creating inner classes of their object; nested objects reached through package paths.

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
  def main(args: Array[String]): Unit = {
    println(Maker.first().v + Maker.make().v)
    println(Maker.many(3))
    println(pkg.Outer.In.f)
  }
}
