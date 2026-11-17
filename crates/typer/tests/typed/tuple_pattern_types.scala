object TuplePatternTypes {
  def patDef = {
    val (x, Some((y: Option[String], z))) = (1, Some((Some("s"), 2))): @unchecked
    (x, y, z)
  }
  def nested = (1, Some((Some("s"), 2))) match {
    case (x, Some((y: Option[String], z))) => y
  }
  def flat(p: (Option[Int], Int)) = p match {
    case t @ (o: Some[Int], y) => (t, o)
    case t @ (o: Option[t], y) => (t, o)
  }
  def annotated(p: (Seq[Option[Int]], Int)) = p match {
    case t @ (s: Seq[Some[Int]] @unchecked, y) => (t, s)
    case t @ (s: Seq[Some[Int] @unchecked], y) => (t, s)
  }
  def wildcard(p: (Any, Int)) = p match {
    case t @ (l: List[?], y) => (t, l)
    case _ => null
  }

  def main(args: Array[String]): Unit = {
    println(patDef)
    println(nested)
    println(flat((Some(1), 2)))
    println(wildcard((List(1), 2)))
  }
}
