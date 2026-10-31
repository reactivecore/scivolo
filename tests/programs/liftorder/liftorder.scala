object liftorder {
  def quote(s: String, legal: String): String = {
    val buf = new java.lang.StringBuilder()
    for (i <- 0 until s.length) {
      val ch: Char = s.charAt(i)
      if (legal.indexOf(ch) > -1) buf.append(ch)
      else {
        val bytes: Array[Byte] = String.valueOf(ch).getBytes("UTF-8")
        for (j <- bytes.indices) {
          buf.append('%')
          buf.append(bytes(j) & 0xff)
        }
      }
    }
    buf.toString
  }

  def firsts(list: Array[String]): String =
    (0 until list.length).map(i => list(i).charAt(0)).mkString

  def shown(list: Array[String]): AnyRef =
    new Object {
      override def toString: String = "shown " + list.length
    }

  def split(rem: Int, x: Long): Long = {
    val (e, b) = {
      if (rem != 0) {
        val (r, beg) = if (rem < 0) (1, 2) else (3, 4)
        (x - r, beg)
      } else (x, 0)
    }
    e * 10 + b
  }

  def main(args: Array[String]): Unit = {
    val list = Array("ab", "cd")
    println(quote("a b", "ab"))
    println(firsts(list))
    println(shown(list))
    println(split(1, 7L))
    println(split(-1, 7L))
    println(split(0, 7L))
  }
}
