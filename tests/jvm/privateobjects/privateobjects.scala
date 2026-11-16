object Itself {
  private object O {
    val k = 1
    def me = O
  }
  def m = O.me.k
}

object Fields {
  private object P {
    val k = 2
    def get = k
  }
  def m = P.get
}

object Privateobjects {
  def names(c: Class[?]): String =
    (c.getDeclaredMethods.map(_.getName) ++ c.getDeclaredFields.map(_.getName)).sorted.mkString(" ")

  def main(args: Array[String]): Unit = {
    println(Itself.m + " " + names(Itself.getClass))
    println(Fields.m + " " + names(Fields.getClass))
  }
}
