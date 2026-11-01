import scala.quoted.*

object Dups:
  def pair(p: (Int, Int)): Int = p match
    case (a, a) => a
  def nested(o: Option[Option[Int]]): Int = o match
    case x @ Some(x) => 1
    case _           => 0
  def types2(o: Any): Int = o match
    case (_: List[t], _: Option[t]) => 1
    case (t: Int, _: List[t])       => 2
    case _                          => 0
  def types(t: (Type[?], Type[?]))(using Quotes): Int = t match
    case ('[a], '[a]) => 1
    case _            => 0
