object CaseMatch {
  def main(args: Array[String]): Unit = {
    val s: shapes.Shape = shapes.Rect(2.0, 3.0)
    s match {
      case shapes.Rect(w, h) => println(w + h)
      case shapes.Dot => println(0)
    }
  }
}
