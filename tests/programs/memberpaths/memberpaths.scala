package cells {
  abstract class Base {
    protected var proxied: Base | Null = null
    protected def label: String = "base"
  }
}

package user {
  final class Sub extends cells.Base {
    def adopt(other: Sub): Unit = other.proxied = this
    def describe(other: Sub): String = other.label + (if other.proxied ne null then "+" else "-")
  }

  trait Shapes {
    def pick[S <: AnyRef](xs: List[S]): S & AnyRef = xs.head
  }

  abstract class Picker extends Shapes {
    def pick[S <: AnyRef](xs: List[S]): S & AnyRef
  }

  final class Last extends Picker {
    override def pick[S <: AnyRef](xs: List[S]): S & AnyRef = xs.last
  }

  object P { val n = 1 }
  object Q { val n = 2 }

  class Shadow {
    val n = 3
    import P._
    import Q._
    def get: Int = n
  }
}

object memberpaths {
  def main(args: Array[String]): Unit = {
    val (a, b) = (new user.Sub, new user.Sub)
    println(a.describe(b))
    a.adopt(b)
    println(a.describe(b))
    println(new user.Last().pick(List("x", "y")))
    println(new user.Shadow().get)
  }
}
