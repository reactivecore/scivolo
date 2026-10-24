//> using scala 3.9.0
import scala.language.experimental.namedTypeArguments

final case class Nt24Entry[K, V](value: V)

object Nt24namedtargs {
  def pair[A, B](a: A, b: B): (A, B) = (a, b)

  def main(args: Array[String]): Unit = {
    val e = Nt24Entry[K = "name"]("foo")
    println(e.value)
    val p = pair[B = Any](1, "two")
    println(p)
    println(pair[B = Long, A = Int](3, 4L))
  }
}
