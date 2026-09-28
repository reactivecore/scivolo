//> using scala 3.9.0

/** JDK methods the VM provides beyond Scala Native: `Thread.getStackTrace` of the current
  * thread and `Runtime`'s memory figures; `jdk.check` is written by hand.
  */
def caller(): String =
  val trace = Thread.currentThread.getStackTrace
  s"${trace(0).getClassName}.${trace(0).getMethodName} ${trace(1).getMethodName} ${trace(2).getMethodName}"

@main def jdk(): Unit =
  println(caller())
  val other = Thread(() => ())
  println(other.getStackTrace.length)
  val rt = Runtime.getRuntime
  val (total, max, free) = (rt.totalMemory, rt.maxMemory, rt.freeMemory)
  println(total > 0 && total <= max && free >= 0 && free <= total)
