import autoapply.Old

object AutoApply {
  def main(args: Array[String]): Unit = {
    val o = new Old
    println(o.size)
    o.run
    println(o.toString)
    println(o.size())
  }
}
