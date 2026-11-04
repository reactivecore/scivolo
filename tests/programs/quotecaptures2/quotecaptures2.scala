import scala.quoted.*

object quotecaptures2 {
  def main(args: Array[String]): Unit = println("lowered")

  def g(x: Expr[Int])(using Quotes): Expr[Int] = x
  def show[T](t: Type[T])(using Quotes): Expr[Int] = '{ 1 }

  def assigned(using Quotes): Expr[Int] = '{
    var n = 1
    ${ g('{ n = n + 2; n }) }
  }

  def poly(using Quotes): Expr[Int] = '{
    def f[A](a: A): A = a
    val k = 3
    ${ g('{ f[Int](k) }) }
  }

  def alias(using Quotes): Expr[Int] = '{
    type T = Int
    ${ show(Type.of[List[T]]) }
  }

  def param(using Quotes): Expr[Int] = '{
    def m[T](x: T): Int = ${ show(Type.of[List[T]]) }
    m("a")
  }

  def nested(using Quotes): Expr[Int] = '{
    def m[T](x: T): Int = ${ g('{ List.empty[T].size }) }
    m(1)
  }
}
