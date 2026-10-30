//> using scala 3.9.0
class Pool:
  var ran = 0
  def runWorker(w: Pool#Worker): Unit = ran += w.id
  final class Worker(val id: Int) extends Runnable:
    override def run(): Unit = runWorker(this)

@main def projectionthis(): Unit =
  val pool = Pool()
  pool.Worker(3).run()
  pool.Worker(4).run()
  println(pool.ran)
