package object pk {
  lazy val AnyRef = new Object {
    override def toString() = "object AnyRef"
  }
}

object pkglazyval {
  def main(args: Array[String]): Unit = println(pk.AnyRef)
}
