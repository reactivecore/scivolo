trait Show[T] { def show(t: T): String }
class ShowList[A](f: A => String) extends Show[List[A]] { def show(xs: List[A]) = xs.map(f).mkString(",") }

object O {
  def use = summon[Show[Int]].show(1)
  def first = lists.show(List(1, 2).map(_ + 1))
  given lists: Show[List[Int]] = xs => xs.map(_.toString).mkString(",")
  given ints: Show[Int] with { def show(t: Int) = t.toString }

  def local = {
    def use = summon[Show[Int]].show(1)
    implicit lazy val inferred = new ShowList((i: Int) => (i + 1).toString)
    given ints: Show[Int] with { def show(t: Int) = t.toString }
    use + inferred.show(List(1, 2))
  }
}
