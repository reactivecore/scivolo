object Trycatch {
  var log = ""

  def parse(s: String): Int =
    try s.toInt
    catch { case e: NumberFormatException => -1 }

  def guarded(i: Int): String =
    try {
      if (i == 0) throw new IllegalStateException("zero")
      "ok " + i
    } catch {
      case e: IllegalStateException => "state " + e.getMessage
      case _: RuntimeException => "runtime"
    } finally {
      log += "g" + i
    }

  def early(i: Int): Int = {
    try {
      if (i > 0) return i * 2
      log += "n"
    } finally {
      log += "f"
    }
    0
  }

  def nested(i: Int): Int =
    try {
      try {
        if (i == 1) throw new RuntimeException("inner")
        i
      } finally {
        log += "i"
      }
    } catch {
      case e: RuntimeException => -i
    } finally {
      log += "o"
    }

  def any(i: Int): String =
    try {
      if (i < 0) throw new Error("neg")
      "pos"
    } catch { case t: Throwable => t.getMessage }

  def unitTry(): Unit =
    try log += "u"
    finally log += "v"

  val lock = new Object
  var count = 0

  def locked(i: Int): Int = lock.synchronized {
    count += i
    count
  }

  def lockedReturn(i: Int): Int = {
    lock.synchronized {
      if (i > 5) return i
      count += 1
    }
    count
  }

  def main(args: Array[String]): Unit = {
    println(Stash.stashed("4") + " " + Stash.stashed("x"))
    println(s"${Stash.widened(2, "1.5")} ${Stash.widened(2, "x")}")
    println(s"${parse("12")} ${parse("x")}")
    println(guarded(0) + "; " + guarded(3))
    println(s"${early(4)} ${early(-1)}")
    println(s"${nested(1)} ${nested(2)}")
    println(any(-1) + " " + any(1))
    unitTry()
    println(s"${locked(2)} ${locked(3)}")
    lock.synchronized { count += 10 }
    println(s"${lockedReturn(7)} ${lockedReturn(1)}")
    println(log)
    println(try { pair(throw new Error("arg"), 1L) } catch { case e: Error => e.getMessage })
  }

  def pair(i: Int, l: Long): Long = i + l
}

object Stash {
  class Pair(val a: Long, val b: String)

  def stashed(s: String): String = {
    val p = new Pair(7L, try s.toInt.toString catch { case _: NumberFormatException => "nan" })
    val n = 1.5 + (try s.toDouble catch { case _: NumberFormatException => 0.0 } finally Trycatch.log += "s")
    p.a.toString + p.b + Math.max(3L, try s.toLong catch { case _: Throwable => 9L }) + n
  }

  def widened(i: Int, s: String): Float = i + (try s.toFloat catch { case _: NumberFormatException => 0.5f })
}
