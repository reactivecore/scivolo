val a = 1
def b = a + 1
Console.flush()
a + b
import scala.collection.mutable.ListBuffer
def sum(xs: List[Int]): Int =
  xs.foldLeft(0)(_ + _)
