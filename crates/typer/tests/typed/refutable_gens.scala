import scala.language.`3.3`
object Main:
  def run(l: List[(Int, Option[Int])]): List[Int] =
    for (a, Some(b)) <- l yield a + b
