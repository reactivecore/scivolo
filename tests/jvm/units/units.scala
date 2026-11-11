import scala.language.`3.7`

object Units {
  val label: Version.Label = (v, suffix) => v.show + suffix

  def main(args: Array[String]): Unit = println(label.of(Version(3, 9), "!"))
}
