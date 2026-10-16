//> using scala 3.9.0
// Sub matches in matches and partial functions.
import scala.language.experimental.subCases

object subcases {
  def m(o: Option[Option[Int]]): String = o match
    case Some(x) if x match
      case Some(1) => "one"
      case None => "none"
    case _ => "other"

  val pf: PartialFunction[Option[Int], Int] =
    case Some(x) if x match
      case 1 => 10
      case 2 => 20

  def main(args: Array[String]): Unit = {
    println(List(Some(Some(1)), Some(None), Some(Some(2)), None).map(m))
    println(List(Some(1), Some(3), None).map(pf.isDefinedAt))
    println(pf(Some(2)))
  }
}
