object NestedRefused {
  def main(args: Array[String]): Unit = {
    println(new refused.C)
    println(refused.AObj.b.n)
  }
}
