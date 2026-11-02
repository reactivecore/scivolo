//> using scala 3.9.0

object objectvarlambda {
  var count = 0
  def run(r: Runnable) = r.run()
  def main(a: Array[String]) = {
    run(() => count += 1)
    run(() => count = 5)
    run(() => println(count))
  }
}
