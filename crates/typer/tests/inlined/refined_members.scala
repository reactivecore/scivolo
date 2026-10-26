import scala.compiletime.erasedValue

trait A:
  type T

inline def kind[T]: String = inline erasedValue[T] match
  case _: Int => "int"
  case _: AnyVal => "anyval"
  case _ => "other"

object Refined:
  def wider(x: A { type T <: AnyVal } & A { type T >: Int }): String = kind[x.T]
  def narrower(x: A { type T <: AnyVal } & A { type T <: Int }): String = kind[x.T]
