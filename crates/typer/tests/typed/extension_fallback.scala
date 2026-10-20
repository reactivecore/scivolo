trait Ord[T] {
  extension (x: T) def compareTo(y: T): Int
}

given Ord[Int] with {
  extension (x: Int) def compareTo(y: Int): Int = x - y
}

class ListOrd[T: Ord] extends Ord[List[T]] {
  extension (xs: List[T]) def compareTo(ys: List[T]): Int = (xs, ys) match {
    case (x :: _, y :: _) => x.compareTo(y)
    case _ => 0
  }
}

object Test {
  def main(args: Array[String]): Unit =
    println(ListOrd[Int]().compareTo(List(1))(List(2)))
}
