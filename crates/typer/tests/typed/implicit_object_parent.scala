class ShowList[T](f: T => String)
object Main {
  implicit object x extends ShowList((i: Int) => i.toString)
  implicit object y extends ShowList[Int](_.toString)
  object z extends ShowList((i: Int) => i.toString)
  given ShowList((i: Int) => i.toString) with {}
  given g: ShowList[Long](_.toString) with {}
}
