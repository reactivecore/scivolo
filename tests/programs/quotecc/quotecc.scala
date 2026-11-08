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

object Twice {
  inline def apply[T](x: T): (T, T) = (x, x)
}

object quotecc {
  def empty(using Quotes): Expr[Tuple] = '{ Tuple() }

  def twice(x: Expr[Int])(using Quotes): Expr[(Int, Int)] = '{ Twice($x) }

  def isTwiceOne(e: Expr[Any])(using Quotes): Boolean = e match {
    case '{ Twice(1) } => true
    case _ => false
  }

  def main(args: Array[String]): Unit = println(args.length)
}
