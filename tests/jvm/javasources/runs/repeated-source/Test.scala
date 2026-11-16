@Note("a") @Note("b") class Twice
@Note("c") class Once

object Test {
  def main(args: Array[String]): Unit = {
    println(classOf[Twice].getAnnotation(classOf[Notes]).value.map(_.value).mkString(","))
    println(classOf[Once].getAnnotations.length)
  }
}
