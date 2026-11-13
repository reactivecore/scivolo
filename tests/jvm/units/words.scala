package units

object Words {
  val shout: String => String = _ + "!"
  def lengths(ws: List[String]): List[Int] = ws.map(_.length).filter(_ > 1)
  def first: Int = (8, 9)._1
  inline def kind(n: Int): String = {
    val m = n % 3
    m match {
      case 0 => "three"
      case _ => "other"
    }
  }
}
