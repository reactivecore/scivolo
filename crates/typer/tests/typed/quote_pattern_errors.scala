import scala.quoted.*

object Errors:
  def f(e: Expr[Any])(using Quotes): Any = e match
    case '{ [A] => (x: A) => $a(x): A }  => 1
    case '{ var v: Int = 1; $g(v): Int } => 2
    case '{ val $y: Int = 1; () }        => 3
    case '{ 1 match { case _ => 2 } }    => 5
    case _                               => 6
