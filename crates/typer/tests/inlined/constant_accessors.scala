object Holder {
  private final val x = 1
  private def y = 2
  private def z: 3 = { println("z"); 3 }
  def w: 4 = { println("w"); 4 }
  final val v = 5
  println("init")
}

class Holder {
  import Holder.*
  inline def sum = x + Holder.x + y + z + Holder.z
  inline def pub = Holder.w + Holder.v + v
}

object Test {
  def main(args: Array[String]): Unit = {
    println((new Holder).sum)
    println((new Holder).pub)
  }
}
