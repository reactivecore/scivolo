package scala.probe

class Chains {
  def chain(xs: List[AnyRef]): Int = {
    val head :: (rest: List[String @unchecked]) = xs: @unchecked
    rest.length + head.hashCode
  }
}
