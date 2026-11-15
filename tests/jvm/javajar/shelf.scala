package other

object Shelf {
  def main(args: Array[String]): Unit = println((new shapes.Box.Crate(): shapes.Shelf).shelf())
}
