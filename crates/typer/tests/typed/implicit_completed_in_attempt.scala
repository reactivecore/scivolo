trait Show[T] { def show(t: T): String }

object O {
  def use = summon[Show[Int]].show(1)
  def first = lists.show(List(1, 2).map(_ + 1))
  given lists: Show[List[Int]] = xs => xs.map(_.toString).mkString(",")
  given ints: Show[Int] with { def show(t: Int) = t.toString }
}
