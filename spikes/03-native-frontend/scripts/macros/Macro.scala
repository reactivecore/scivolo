import scala.quoted.*

object Macros:
  inline def power(x: Double, inline n: Int): Double = ${ powerCode('x, 'n) }

  def powerCode(x: Expr[Double], n: Expr[Int])(using Quotes): Expr[Double] =
    def loop(k: Int): Expr[Double] =
      if k == 0 then '{ 1.0 }
      else if k % 2 == 0 then '{ val y = ${ loop(k / 2) }; y * y }
      else '{ $x * ${ loop(k - 1) } }
    loop(n.valueOrAbort)

  inline def debug(inline e: Any): String = ${ debugCode('e) }

  def debugCode(e: Expr[Any])(using Quotes): Expr[String] =
    val src = Expr(e.show)
    '{ $src + " = " + $e }

  inline def kind[T]: String = ${ kindCode[T] }

  def kindCode[T: Type](using Quotes): Expr[String] =
    Type.of[T] match
      case '[Int]    => Expr("Int")
      case '[String] => Expr("String")
      case '[t]      => Expr("other: " + Type.show[t])

  inline def listElem[T]: String = ${ listElemCode[T] }

  def listElemCode[T: Type](using Quotes): Expr[String] =
    Type.of[T] match
      case '[List[t]] => Expr("List of " + Type.show[t])
      case _          => Expr("not a list")
