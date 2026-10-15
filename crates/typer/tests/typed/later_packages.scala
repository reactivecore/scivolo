import bip.*

object Test {
  def main(args: Array[String]): Unit = {
    println(new foo.Bar)
    println(foo.baz.Qux.n + Top.n)
  }
}

package foo {
  class Bar
  package baz {
    object Qux { val n = 1 }
  }
}

package bip {
  object Top { val n = 2 }
}
