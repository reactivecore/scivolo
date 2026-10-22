//> using scala 3.9.0
object handlerexprs {
  class Boom extends Exception("boom")
  class Other extends Exception("other")

  def raise(x: Int): Int = { throw new Exception(s"$x"); 0 }
  def handle: Throwable => Int = { case ex: Exception => ex.getMessage.toInt }

  val onlyBoom: PartialFunction[Throwable, String] = { case b: Boom => "caught " + b.getMessage }

  def risky(boom: Boolean): String =
    try { if (boom) throw new Boom else throw new Other }
    catch onlyBoom

  def main(args: Array[String]): Unit = {
    println(try raise(7) catch handle)
    println(risky(true))
    try println(risky(false)) catch { case o: Other => println("rethrown " + o.getMessage) }
    val inferred = try raise(1) catch (e => e.getMessage.length + 41)
    println(inferred + 1)
  }
}
