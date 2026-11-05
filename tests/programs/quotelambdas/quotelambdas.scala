import scala.quoted.*

object quotelambdas {
  def main(args: Array[String]): Unit = println(args.length)

  def plus(using Quotes): Expr[Int => Int] = '{ (_: Int) + 1 }

  def sizes(using Quotes): Expr[(List[Int], Int) => Int] = '{ (_: List[Int]).size * (_: Int) }

  def lengths(using Quotes): Expr[(String, scala.Int, Array[Int]) => Int] =
    '{ (_: String).length * (_: scala.Int) + (_: Array[Int]).length }

  def shown(using Quotes): Expr[String] =
    '{
      def f[T](x: T): String = ${ Expr(Type.show[List[T]]) } + x
      f[Int](1)
    }
}
