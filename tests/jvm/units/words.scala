package units

object Words {
  val shout: String => String = _ + "!"
  def lengths(ws: List[String]): List[Int] = ws.map(_.length).filter(_ > 1)
  def first: Int = (8, 9)._1
}
