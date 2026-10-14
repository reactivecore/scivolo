object Main:
  def run(l: List[(Int, Option[Int])]): List[Int] =
    for case (a, Some(b)) <- l yield a + b
