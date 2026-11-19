object Test:
  def combine(f: (Char, Int) => Int): Int = 1
  def combine(f: (String, Int) => String): String = "2"
  val r5 = combine((x: Char, y) => x + y)
  val t5: Int = r5
  val r6 = combine((x: String, y) => x ++ y.toString)
  val t6: String = r6
