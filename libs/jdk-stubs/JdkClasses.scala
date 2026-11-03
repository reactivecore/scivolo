// JDK classes javalib does not define that library sources import. Every frontend's library has
// their TASTy without NIR (ADR 0072), so a use of one fails to link, as in Scala Native.

package java.io {
  class Console extends Flushable {
    def writer(): PrintWriter = null
    def reader(): Reader = null
    def readLine(): String = null
    def flush(): Unit = ()
  }

  class SequenceInputStream(s1: InputStream, s2: InputStream) extends InputStream {
    def this(e: java.util.Enumeration[? <: InputStream]) = this(null, null)
    override def available(): Int = 0
    def read(): Int = 0
    override def read(b: Array[Byte], off: Int, len: Int): Int = 0
    override def close(): Unit = ()
    override def transferTo(out: OutputStream): Long = 0L
  }
}

package java.lang.reflect {
  final class Field extends AccessibleObject { def get(obj: Object): Object = null }
  final class Method extends Executable {
    def getName(): String = null
    def getParameterTypes(): scala.Array[Class[?]] = null
  }
}

package java.net {
  final class URL(spec: String) {
    final def openStream(): java.io.InputStream = null
  }
}

package sun.nio.cs {
  final class StreamEncoder private () extends java.io.Writer {
    def write(cbuf: Array[Char], off: Int, len: Int): Unit = ()
    def flush(): Unit = ()
    def close(): Unit = ()
  }
}
