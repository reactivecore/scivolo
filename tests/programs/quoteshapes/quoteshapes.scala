import scala.quoted.*

object quoteshapes {
  def main(args: Array[String]): Unit = println(args.length)

  def options(using Quotes): Expr[Option[List[Int]] => Int] = '{ (_: Option[List[Int]]).size }

  def options2(using Quotes): Expr[Option[List[Int]] => Int] = '{ (o: Option[List[Int]]) => o.size }

  def qualified(using Quotes): Expr[scala.collection.immutable.List[Int] => Int] =
    '{ (_: scala.collection.immutable.List[Int]).size }

  def qualified2(using Quotes): Expr[scala.collection.immutable.List[Int] => Int] =
    '{ (l: scala.collection.immutable.List[Int]) => l.size }

  def unions(using Quotes): Expr[(Int | String) => String] = '{ (_: Int | String).toString }

  def unions2(using Quotes): Expr[(Int | String) => String] = '{ (u: Int | String) => u.toString }

  def size[T: Type](xs: Expr[List[T]])(using Quotes): Expr[Int] = '{ $xs.size }

  def nested[T: Type](using Quotes): Expr[List[T] => Int] = '{ (xs: List[T]) => ${ size[T]('xs) } }

  def holeInt(x: Expr[Int])(using Quotes): Expr[Int] = '{ val y: Int = $x; y + 1 }

  def nested2[T: Type](xs: Expr[List[T]])(using Quotes): Expr[Int] =
    '{ val ys: List[T] = $xs; ${ size[T]('ys) } + ys.length }

  def nested3[T: Type](xs: Expr[List[T]])(using Quotes): Expr[Int] =
    '{ $xs.map(x => List(x)).map(l => ${ size[T]('l) }).sum }

  def twice(x: Expr[Int])(using Quotes): Expr[Int] = '{ ${ holeInt('{ $x + 1 }) } * 2 }

  def intArg(xs: Expr[List[Int]])(using Quotes): Expr[Int] = '{ val l = $xs; ${ size[Int]('l) } + 1 }

  def typeArg[T: Type](xs: Expr[List[T]])(using Quotes): Expr[Int] =
    '{ def g[U](us: List[U]): Int = ${ size[U]('us) }; g[T]($xs) }

  def splitArgs[T: Type](using Quotes): Expr[(List[T], List[Int]) => Int] =
    '{ (a: List[T], b: List[Int]) => ${ size[T]('a) } + ${ size[Int]('b) } }

  def shownType(using Quotes): Expr[String] =
    '{ def g[U](u: U): String = ${ Expr(Type.show[U]) }; g(1) }

  def shownBoth(using Quotes): Expr[String] =
    '{ def g[U](u: U): String = ${ Expr(Type.show[U] + 'u.show) }; g(1) }

  def unionOpt(using Quotes): Expr[Option[Int | String] => Int] = '{ (o: Option[Int | String]) => o.size }
}
