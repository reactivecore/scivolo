import scala.collection.mutable

class Counter:
  var n = 0
  def bump(): Unit = n += 2

object Main:
  var total = 0L
  def run(): Unit =
    var acc = 0L
    var i = 1
    acc += i
    i *= 3
    i -= 1
    var s = "a"
    s += "b"
    s += 1
    total += acc
    val c = new Counter
    c.n += 5
    val buf = mutable.ArrayBuffer.empty[Int]
    buf += 1
    var set = Set(1)
    set += 2
    var d = 1.5
    d /= 2
    var b = 3
    b <<= 1
    b |= 4
    println(s"$acc $i $s $total ${c.n} $buf $set $d $b")
