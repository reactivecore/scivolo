import scala.quoted.*

trait Base[T] { def put(t: T): Unit }
trait Outer {
  trait Mid[V] extends Base[V]
  trait Inner[V] extends Mid[V] { def length(v: V): Int }
}

object quotemembers {
  def main(args: Array[String]): Unit = println("lowered")

  type Mapped[F[_]] = [X <: Tuple] =>> X =:= Tuple.Map[Tuple.InverseMap[X, F], F]

  def pair(a: Expr[AnyRef], b: Expr[Int])(using Quotes): Expr[(AnyRef, Int)] = Expr.ofTuple((a, b))
  def curried = summon[Mapped[Option][(Option[Int], Option[String])]]

  def flags(t: Any)(using Quotes): Expr[List[Boolean]] = {
    import quotes.reflect.*
    val c = t.asInstanceOf[Constant]
    Expr.ofList(Some(c).map { case BooleanConstant(b) => b }.map(Expr(_)).toList)
  }

  def tree(arg: Any)(using Quotes) = {
    import quotes.reflect.*
    val a = arg.asInstanceOf[TypeRepr]
    TypeTree.of(using AppliedType(a, List(a)).asType)
  }

  def inner[T: Type](self: Expr[Outer#Inner[T]], v: Expr[T])(using Quotes): Expr[Int] =
    '{ ${ self }.length(${ v }) }
  def mid(self: Expr[Outer#Mid[Int]])(using Quotes): Expr[Unit] = '{ ${ self }.put(1) }
}
