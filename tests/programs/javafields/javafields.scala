class B(in: java.io.InputStream) extends java.io.BufferedInputStream(in) {
  def f = buf.length + count + pos
  def g(): Unit = { pos = 1; markpos = count }
}

object Interrupted extends java.io.InterruptedIOException {
  def h = { bytesTransferred = 5; bytesTransferred }
}

object javafields {
  def main(args: Array[String]): Unit = {
    val b = new B(new java.io.ByteArrayInputStream(Array[Byte](1, 2, 3)))
    b.read()
    b.g()
    println(b.f + Interrupted.h)
  }
}
