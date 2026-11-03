import scala.quoted.*

class Opts(val a: Int = 1, val b: String = "b")
class Inv[T](val t: T)

object quoteholes {
  def main(args: Array[String]): Unit = {
    val m = if (args.isEmpty) Map("k" -> 1) else Map.empty
    println(m.get("k"))
  }

  def defaults(using Quotes): Expr[Opts] = '{ new Opts() }

  def holes[B: Type](inv: Expr[Inv[B]], f: Expr[B => Int], o: Expr[Option[B]])(using Quotes): Expr[Int] =
    '{ $f(${ o }.get) + ${ inv }.hashCode }

  def refined[B: Type](e: Expr[Opts { def extra(b: B): Int }])(using Quotes): Expr[Int] =
    '{ ${ e }.a }
}
