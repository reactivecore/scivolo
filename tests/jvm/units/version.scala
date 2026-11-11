import scala.language.implicitConversions

case class Version private (major: Int, minor: Int) {
  def show: String = s"$major.$minor"
}

object Version {
  def apply(major: Int, minor: Int): Version = new Version(major, minor)

  trait Label {
    def of(v: Version, suffix: String): String
  }
}
