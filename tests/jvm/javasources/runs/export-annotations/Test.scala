class Wrapper(s: Source) {
  export s.size
}

object Test {
  def main(args: Array[String]): Unit = {
    val size = classOf[Wrapper].getMethod("size")
    println(size.getAnnotation(classOf[Tag]).value)
    println(Wrapper(Source()).size())
  }
}
