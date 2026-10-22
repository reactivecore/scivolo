//> using scala 3.9.0
object jdkapi extends Exception("e") {
  def restack(t: Throwable): Int = {
    t.setStackTrace(t.getStackTrace)
    t.getStackTrace.length
  }

  def main(args: Array[String]): Unit = {
    println(getMessage)
    println(restack(new Exception) >= 0)
  }
}
