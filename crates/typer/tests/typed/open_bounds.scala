trait Show[A]

object Prefix:
  type Bounded >: String <: String
  given C: Show[Bounded]()

object Main {
  def view[B](implicit ev: Int => B): Int = 1
  val a = List(List(1), List(2)).flatten
  val b = Seq((1, "a")).unzip
  val c = Map(1 -> 1).toMap
  val d = view
  val e = (1L to 10L).sum
  val f = summon[Show[Prefix.Bounded]]
}
