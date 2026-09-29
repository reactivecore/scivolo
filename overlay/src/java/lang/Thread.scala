package java.lang

/** The current thread's frames, the way the JDK lists them: `Thread.getStackTrace` first. */
abstract class Thread {
  def getStackTrace(): Array[StackTraceElement] =
    if (this eq Thread.currentThread()) new Throwable().getStackTrace()
    else new Array[StackTraceElement](0)
}

object Thread {
  def currentThread(): Thread = ???
}
