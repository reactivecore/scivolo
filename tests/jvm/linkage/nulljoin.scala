package nulljoin

import java.io.IOException

object Main {
  def none: Null = null

  def branches(c: Boolean, e: Throwable): Object = {
    val kept = if (c) none else throw e
    kept
  }

  def nested(a: Boolean, b: Boolean): String = if (a) (if (b) none else none) else none

  def guarded(read: () => String, retry: Boolean): String =
    try read()
    catch {
      case _: InterruptedException => none
      case _: IOException if retry => none
    }

  def main(args: Array[String]): Unit = {
    println(branches(true, new Exception))
    println(nested(true, false))
    println(guarded(() => throw new IOException, true))
    println(guarded(() => "read", false))
  }
}
