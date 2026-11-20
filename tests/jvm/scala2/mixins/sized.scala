class Measured extends mixins.Sized

object SizedMain {
  def main(args: Array[String]): Unit = println(new Measured().sizes(List(1, 2)))
}
