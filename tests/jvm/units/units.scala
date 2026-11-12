import scala.language.`3.7`

class Release(val major: Major, val minor: Int) {
  def this(minor: Minor, major: Int) = this(Major.of(major), 0)
}

object Units {
  val label: Version.Label = (v, suffix) => v.show + suffix

  def main(args: Array[String]): Unit = {
    println(label.of(Version(3, 9), "!"))
    println(new Release(Major.of(3), 9).major.n)
    println(new units.text.Line().of(3))
    println(Thread("x"))
    println(new Record(2).n)
    println(io.Top.name)
    println(units.Labels.top)
  }
}
