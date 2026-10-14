import scala.language.`3.7`
object Q:
  def unapply(x: Int): Option[Int] = Some(x)

object Main:
  def run(o: Option[Int], n: Int): Int =
    val Some(y) = o
    val Q(c) = n
    y + c
