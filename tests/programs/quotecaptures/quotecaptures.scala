import scala.quoted.*

object quotecaptures {
  def main(args: Array[String]): Unit = println("lowered")

  def g(x: Expr[Int])(using Quotes): Expr[Int] = x
  def twice(x: Expr[Int])(using Quotes): Expr[Int] = '{ $x * 2 }
  def h(x: Expr[Int], y: Expr[String])(using Quotes): Expr[Int] = '{ $x + $y.length }

  def quoted(using Quotes): Expr[Int] = '{ val y = 21; ${ twice('y) } }

  def two(using Quotes): Expr[Int] =
    '{ val a = 3; val b = "four"; ${ h('a, 'b) } + ${ g('{ a + 100 }) } }

  def applied(using Quotes): Expr[Int] = '{
    def end(a: Int, b: Boolean): Int = if b then a * 10 else 0
    val k = 3
    ${ g('{ end(k + 1, true) }) }
  }

  def curried(using Quotes): Expr[Int] = '{
    def add(a: Int)(b: Int): Int = a * 100 + b
    val c = 3
    ${ twice('{ add(c)({ val d = c + 1; d }) }) }
  }

  def deep(using Quotes): Expr[Int] = '{ val y = 7; ${ g('{ val z = 5; ${ twice('{ z + y }) } }) } }

  def byName(using Quotes): Expr[Int] = '{
    var n = 0
    def f(x: => Int): Int = ${ g('{ x * 10 + x }) }
    f({ n = n + 1; n })
  }

  def lambda(using Quotes): Expr[Int => Int] = '{ (p: Int) => ${ twice('{ p + 1 }) } }

  def cancelled(x: Expr[Int])(using Quotes): Expr[Int] = '{ ${ x } }

  def unit(using Quotes): Expr[Unit] = '{}

  def nul(using Quotes): Expr[String] = '{ null }
}
