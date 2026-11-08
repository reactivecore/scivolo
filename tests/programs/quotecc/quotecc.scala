import language.experimental.captureChecking
import scala.quoted.*

trait Lift[T] { def apply(x: T)(using Quotes): Expr[T] }

class SomeLift[T: Type: Lift] extends Lift[Some[T]] {
  def apply(x: Some[T])(using Quotes): Expr[Some[T]] = '{ Some[T](${ summon[Lift[T]](x.get) }) }
}

class ConsLift[H: Type, T <: Tuple: Type] {
  def apply(head: Expr[H], tail: Expr[T])(using Quotes): Expr[H *: T] = '{ $head *: $tail }
}

class ListLift[T: Lift] {
  def apply(xs: List[T])(using Quotes): List[Expr[T]] = xs.map(summon[Lift[T]].apply)
}

object quotecc {
  def empty(using Quotes): Expr[Tuple] = '{ Tuple() }

  def main(args: Array[String]): Unit = println(args.length)
}
