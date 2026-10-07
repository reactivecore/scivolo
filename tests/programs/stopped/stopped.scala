//> using scala 3.9.0

object Locked:
  private val lock = Object()

  def spin(): String = lock.synchronized {
    lock.synchronized {
      while true do ()
      "unreachable"
    }
  }

  def take(): String =
    var got = "blocked"
    val t = Thread(() => lock.synchronized { got = "taken" })
    t.start()
    t.join()
    got

@main def stopped(): Unit = println(Locked.take())
