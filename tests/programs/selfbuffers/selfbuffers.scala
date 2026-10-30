abstract class Buf(val capacity: Int) {
  type Elem

  type BufType >: this.type <: Buf {
    type Elem = Buf.this.Elem
  }

  def arr: Array[Elem] = null
  def load(i: Int): Elem
  def store(i: Int, e: Elem): Unit
  def store(i: Int, src: Array[Elem], n: Int): Unit
}

final class Gen[B <: Buf](val self: B) extends AnyVal {
  import self._

  def copyFrom(src: BufType): BufType = {
    val srcArr = src.arr
    if (srcArr != null) store(0, srcArr, capacity)
    else {
      var i = 0
      while (i < capacity) { store(i, src.load(i)); i += 1 }
    }
    self
  }
}

final class IntBuf(n: Int, override val arr: Array[Int]) extends Buf(n) {
  type Elem = Int
  type BufType = IntBuf

  def load(i: Int): Int = arr(i)
  def store(i: Int, e: Int): Unit = arr(i) = e
  def store(i: Int, src: Array[Int], n: Int): Unit = System.arraycopy(src, 0, arr, i, n)
  def put(src: IntBuf) = Gen[IntBuf](this).copyFrom(src)
}

final class ListBuf(n: Int) extends Buf(n) {
  type Elem = String
  type BufType = ListBuf
  private val items = new Array[String](n)

  def load(i: Int): String = items(i)
  def store(i: Int, e: String): Unit = items(i) = e
  def store(i: Int, src: Array[String], n: Int): Unit = System.arraycopy(src, 0, items, i, n)
  def put(src: ListBuf) = Gen[ListBuf](this).copyFrom(src)
}

@main def selfbuffers(): Unit = {
  val a = new IntBuf(3, Array(1, 2, 3))
  val b = new IntBuf(3, new Array[Int](3))
  val c: IntBuf = b.put(a)
  println(c.arr.mkString(","))
  val l = new ListBuf(2)
  l.store(0, "x"); l.store(1, "y")
  val m = new ListBuf(2)
  println(m.put(l).load(1))
}
