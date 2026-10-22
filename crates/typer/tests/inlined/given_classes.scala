trait Show[A]:
  def show(a: A): String

inline given Show[Int]:
  def show(a: Int): String = s"int $a"

inline given showList: [A] => (s: Show[A]) => Show[List[A]]:
  def show(as: List[A]): String = as.map(s.show).mkString(",")

object Test:
  def main(args: Array[String]): Unit =
    println(summon[Show[Int]].show(1))
    println(summon[Show[List[Int]]].show(List(1, 2)))
