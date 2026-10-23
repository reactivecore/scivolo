trait TDBox:
  def get(): Int

object TDOps:
  extension [T](xs: List[T])
    def plus(y: Int): TDBox = new:
      def get() = xs.length + y
    def times(y: Int): TDBox = new:
      def get() = xs.length * y

object TDextshared:
  import TDOps.*
  def main(args: Array[String]): Unit =
    println(List(1, 2, 3).plus(4).get())
    println(List("a", "b").times(4).get())
