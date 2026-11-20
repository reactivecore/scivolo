trait Fn[-T, R] { def apply(t: T): R }

object ClosureParamSources {
  def sum(x: Int, y: => Int) = x + y
  val byName = sum(2, _)

  def total(i: Int*) = i.sum
  val repeated = i => total(i*)

  val wildcard: Function1[?, ?] = x => 1
  val wildcardResult: Function1[?, Nothing] = x => x

  type T2 = Tuple2[Int, Int]
  val tupledAlias: T2 => Int = (x1, x2) => x1 + x2

  val partial: PartialFunction[?, Any] = { case i => i }

  val refined: Fn[Int, Int] { def apply(arg: Int): Int } = x => x + 1

  val unit = () => ( 234 )
}
