import scala.reflect.TypeTest

object Main:
  type U[X] = X | String
  def union[T](x: Any)(using TypeTest[Any, U[T]]): Int = x match
    case _: U[T] => 1
    case _ => 2
