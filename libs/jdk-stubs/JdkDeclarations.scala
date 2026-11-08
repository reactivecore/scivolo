// The JDK's declarations of classes javalib also defines, and javalib-intf's Java classes. nativelib's
// sources in `just lib-compile` see these classes in place of javalib's; every other compile sees
// each member declared here in place of javalib's members of its name (ADR 0072), so a name has
// all the JDK's overloads of its class. Java's static members are `@static` (`Long.hashCode`,
// which its class also has, a member of the object), its final fields `val`s and other fields
// `var`s.
import scala.annotation.static

package java.lang {
  final class Boolean extends java.io.Serializable
  object Boolean {
    @static val TYPE: Class[Boolean] = null
    @static val TRUE: Boolean = null
    @static val FALSE: Boolean = null
  }

  final class Byte extends Number {
    def intValue(): Int = 0
    def longValue(): scala.Long = 0L
    def floatValue(): scala.Float = 0f
    def doubleValue(): scala.Double = 0d
  }
  object Byte {
    @static val TYPE: Class[Byte] = null
    @static final val MIN_VALUE: scala.Byte = -128
    @static final val MAX_VALUE: scala.Byte = 127
    @static final val SIZE = 8
    @static final val BYTES = 1
    @static def toUnsignedInt(x: scala.Byte): Int = 0
  }

  final class Short extends Number {
    def intValue(): Int = 0
    def longValue(): scala.Long = 0L
    def floatValue(): scala.Float = 0f
    def doubleValue(): scala.Double = 0d
  }
  object Short {
    @static val TYPE: Class[Short] = null
    @static final val MIN_VALUE: scala.Short = -32768
    @static final val MAX_VALUE: scala.Short = 32767
    @static final val SIZE = 16
    @static final val BYTES = 2
  }

  final class Character extends java.io.Serializable
  object Character {
    @static val TYPE: Class[Character] = null
    @static final val MIN_RADIX = 2
    @static final val MAX_RADIX = 36
    @static final val MIN_VALUE = '\u0000'
    @static final val MAX_VALUE = '￿'
    @static final val MIN_HIGH_SURROGATE = '\ud800'
    @static final val MAX_HIGH_SURROGATE = '\udbff'
    @static final val MIN_LOW_SURROGATE = '\udc00'
    @static final val MAX_LOW_SURROGATE = '\udfff'
    @static final val MIN_SURROGATE = '\ud800'
    @static final val MAX_SURROGATE = '\udfff'
    @static final val MIN_SUPPLEMENTARY_CODE_POINT = 0x10000
    @static final val MIN_CODE_POINT = 0
    @static final val MAX_CODE_POINT = 0x10ffff
    @static final val SIZE = 16
    @static final val BYTES = 2
    @static final val UNASSIGNED: scala.Byte = 0
    @static final val UPPERCASE_LETTER: scala.Byte = 1
    @static final val LOWERCASE_LETTER: scala.Byte = 2
    @static final val TITLECASE_LETTER: scala.Byte = 3
    @static final val MODIFIER_LETTER: scala.Byte = 4
    @static final val OTHER_LETTER: scala.Byte = 5
    @static final val NON_SPACING_MARK: scala.Byte = 6
    @static final val ENCLOSING_MARK: scala.Byte = 7
    @static final val COMBINING_SPACING_MARK: scala.Byte = 8
    @static final val DECIMAL_DIGIT_NUMBER: scala.Byte = 9
    @static final val LETTER_NUMBER: scala.Byte = 10
    @static final val OTHER_NUMBER: scala.Byte = 11
    @static final val SPACE_SEPARATOR: scala.Byte = 12
    @static final val LINE_SEPARATOR: scala.Byte = 13
    @static final val PARAGRAPH_SEPARATOR: scala.Byte = 14
    @static final val CONTROL: scala.Byte = 15
    @static final val FORMAT: scala.Byte = 16
    @static final val PRIVATE_USE: scala.Byte = 18
    @static final val SURROGATE: scala.Byte = 19
    @static final val DASH_PUNCTUATION: scala.Byte = 20
    @static final val START_PUNCTUATION: scala.Byte = 21
    @static final val END_PUNCTUATION: scala.Byte = 22
    @static final val CONNECTOR_PUNCTUATION: scala.Byte = 23
    @static final val OTHER_PUNCTUATION: scala.Byte = 24
    @static final val MATH_SYMBOL: scala.Byte = 25
    @static final val CURRENCY_SYMBOL: scala.Byte = 26
    @static final val MODIFIER_SYMBOL: scala.Byte = 27
    @static final val OTHER_SYMBOL: scala.Byte = 28
    @static final val INITIAL_QUOTE_PUNCTUATION: scala.Byte = 29
    @static final val FINAL_QUOTE_PUNCTUATION: scala.Byte = 30
    @static final val DIRECTIONALITY_UNDEFINED: scala.Byte = -1
    @static final val DIRECTIONALITY_LEFT_TO_RIGHT: scala.Byte = 0
    @static final val DIRECTIONALITY_RIGHT_TO_LEFT: scala.Byte = 1
    @static final val DIRECTIONALITY_RIGHT_TO_LEFT_ARABIC: scala.Byte = 2
    @static final val DIRECTIONALITY_EUROPEAN_NUMBER: scala.Byte = 3
    @static final val DIRECTIONALITY_EUROPEAN_NUMBER_SEPARATOR: scala.Byte = 4
    @static final val DIRECTIONALITY_EUROPEAN_NUMBER_TERMINATOR: scala.Byte = 5
    @static final val DIRECTIONALITY_ARABIC_NUMBER: scala.Byte = 6
    @static final val DIRECTIONALITY_COMMON_NUMBER_SEPARATOR: scala.Byte = 7
    @static final val DIRECTIONALITY_NONSPACING_MARK: scala.Byte = 8
    @static final val DIRECTIONALITY_BOUNDARY_NEUTRAL: scala.Byte = 9
    @static final val DIRECTIONALITY_PARAGRAPH_SEPARATOR: scala.Byte = 10
    @static final val DIRECTIONALITY_SEGMENT_SEPARATOR: scala.Byte = 11
    @static final val DIRECTIONALITY_WHITESPACE: scala.Byte = 12
    @static final val DIRECTIONALITY_OTHER_NEUTRALS: scala.Byte = 13
    @static final val DIRECTIONALITY_LEFT_TO_RIGHT_EMBEDDING: scala.Byte = 14
    @static final val DIRECTIONALITY_LEFT_TO_RIGHT_OVERRIDE: scala.Byte = 15
    @static final val DIRECTIONALITY_RIGHT_TO_LEFT_EMBEDDING: scala.Byte = 16
    @static final val DIRECTIONALITY_RIGHT_TO_LEFT_OVERRIDE: scala.Byte = 17
    @static final val DIRECTIONALITY_POP_DIRECTIONAL_FORMAT: scala.Byte = 18
    @static final val DIRECTIONALITY_LEFT_TO_RIGHT_ISOLATE: scala.Byte = 19
    @static final val DIRECTIONALITY_RIGHT_TO_LEFT_ISOLATE: scala.Byte = 20
    @static final val DIRECTIONALITY_FIRST_STRONG_ISOLATE: scala.Byte = 21
    @static final val DIRECTIONALITY_POP_DIRECTIONAL_ISOLATE: scala.Byte = 22
    @static def charCount(codePoint: Int): Int = 0
    @static def isLowerCase(ch: Char): scala.Boolean = false
    @static def isLowerCase(codePoint: Int): scala.Boolean = false
    @static def isUpperCase(ch: Char): scala.Boolean = false
    @static def isUpperCase(codePoint: Int): scala.Boolean = false
    @static def isTitleCase(ch: Char): scala.Boolean = false
    @static def isTitleCase(codePoint: Int): scala.Boolean = false
    @static def toTitleCase(ch: Char): Char = ch
    @static def toTitleCase(codePoint: Int): Int = 0
    @static def getNumericValue(ch: Char): Int = 0
    @static def getNumericValue(codePoint: Int): Int = 0
    @static def getDirectionality(ch: Char): scala.Byte = 0
    @static def getDirectionality(codePoint: Int): scala.Byte = 0
    @static def codePointAt(seq: CharSequence, index: Int): Int = 0
    @static def codePointAt(a: Array[Char], index: Int): Int = 0
    @static def codePointAt(a: Array[Char], index: Int, limit: Int): Int = 0
    @static def codePointBefore(seq: CharSequence, index: Int): Int = 0
    @static def codePointBefore(a: Array[Char], index: Int): Int = 0
    @static def codePointBefore(a: Array[Char], index: Int, start: Int): Int = 0
    @static def toChars(codePoint: Int): Array[Char] = null
    @static def toChars(codePoint: Int, dst: Array[Char], dstIndex: Int): Int = 0
  }

  final class Integer extends Number {
    def intValue(): Int = 0
    def longValue(): scala.Long = 0L
    def floatValue(): scala.Float = 0f
    def doubleValue(): scala.Double = 0d
  }
  object Integer {
    @static val TYPE: Class[Integer] = null
    @static final val MIN_VALUE = -2147483648
    @static final val MAX_VALUE = 2147483647
    @static final val SIZE = 32
    @static final val BYTES = 4
    @static def toHexString(i: Int): String = null
    @static def compare(x: Int, y: Int): Int = 0
    @static def compareUnsigned(x: Int, y: Int): Int = 0
    @static def toUnsignedLong(x: Int): scala.Long = 0L
    @static def toUnsignedString(i: Int): String = null
    @static def toUnsignedString(i: Int, radix: Int): String = null
    @static def valueOf(i: Int): Integer = null
    @static def valueOf(s: String): Integer = null
    @static def valueOf(s: String, radix: Int): Integer = null
  }

  final class Long extends Number {
    def intValue(): Int = 0
    def longValue(): scala.Long = 0L
    def floatValue(): scala.Float = 0f
    def doubleValue(): scala.Double = 0d
  }
  object Long {
    @static val TYPE: Class[Long] = null
    @static final val MIN_VALUE = -9223372036854775808L
    @static final val MAX_VALUE = 9223372036854775807L
    @static final val SIZE = 64
    @static final val BYTES = 8
    @static def toHexString(i: scala.Long): String = null
    def hashCode(value: scala.Long): Int = 0
    @static def compare(x: scala.Long, y: scala.Long): Int = 0
    @static def compareUnsigned(x: scala.Long, y: scala.Long): Int = 0
    @static def toUnsignedString(i: scala.Long): String = null
    @static def toUnsignedString(i: scala.Long, radix: Int): String = null
    @static def parseUnsignedLong(s: String): scala.Long = 0L
    @static def parseUnsignedLong(s: String, radix: Int): scala.Long = 0L
    @static def parseUnsignedLong(s: CharSequence, beginIndex: Int, endIndex: Int, radix: Int): scala.Long = 0L
  }

  final class Float extends Number {
    def intValue(): Int = 0
    def longValue(): scala.Long = 0L
    def floatValue(): scala.Float = 0f
    def doubleValue(): scala.Double = 0d
  }
  object Float {
    @static val TYPE: Class[Float] = null
    @static final val POSITIVE_INFINITY = 1.0f / 0.0f
    @static final val NEGATIVE_INFINITY = -1.0f / 0.0f
    @static final val NaN = 0.0f / 0.0f
    @static final val MAX_VALUE = 3.4028235E38f
    @static final val MIN_NORMAL = 1.1754944E-38f
    @static final val MIN_VALUE = 1.4E-45f
    @static final val SIZE = 32
    @static final val PRECISION = 24
    @static final val MAX_EXPONENT = 127
    @static final val MIN_EXPONENT = -126
    @static final val BYTES = 4
  }

  final class Double extends Number {
    def intValue(): Int = 0
    def longValue(): scala.Long = 0L
    def floatValue(): scala.Float = 0f
    def doubleValue(): scala.Double = 0d
  }
  object Double {
    @static val TYPE: Class[Double] = null
    @static final val POSITIVE_INFINITY = 1.0d / 0.0d
    @static final val NEGATIVE_INFINITY = -1.0d / 0.0d
    @static final val NaN = 0.0d / 0.0d
    @static final val MAX_VALUE = 1.7976931348623157E308d
    @static final val MIN_NORMAL = 2.2250738585072014E-308d
    @static final val MIN_VALUE = 4.9E-324d
    @static final val SIZE = 64
    @static final val PRECISION = 53
    @static final val MAX_EXPONENT = 1023
    @static final val MIN_EXPONENT = -1022
    @static final val BYTES = 8
  }

  final class Void
  object Void { @static val TYPE: Class[Void] = null }

  final class StringBuilder(capacity: Int) extends AbstractStringBuilder with CharSequence {
    def this() = this(16)
    def this(str: String) = this(16)
    override def append(obj: Object): StringBuilder = this
    override def append(str: String): StringBuilder = this
    override def append(sb: StringBuffer): StringBuilder = this
    override def append(s: CharSequence): StringBuilder = this
    override def append(s: CharSequence, start: Int, end: Int): StringBuilder = this
    override def append(str: Array[Char]): StringBuilder = this
    override def append(str: Array[Char], offset: Int, len: Int): StringBuilder = this
    override def append(b: scala.Boolean): StringBuilder = this
    override def append(c: Char): StringBuilder = this
    override def append(i: Int): StringBuilder = this
    override def append(lng: scala.Long): StringBuilder = this
    override def append(f: scala.Float): StringBuilder = this
    override def append(d: scala.Double): StringBuilder = this
    override def appendCodePoint(codePoint: Int): StringBuilder = this
    override def indexOf(str: String): Int = 0
    override def indexOf(str: String, fromIndex: Int): Int = 0
    override def lastIndexOf(str: String): Int = 0
    override def lastIndexOf(str: String, fromIndex: Int): Int = 0
    override def toString(): String = null
  }

  final class StackTraceElement(declaringClass: String, methodName: String, fileName: String, lineNumber: Int)
      extends java.io.Serializable {
    def getClassName(): String = null
    def getMethodName(): String = null
    override def toString(): String = null
  }

  class Throwable(message: String, cause: Throwable) extends java.io.Serializable {
    def this() = this(null, null)
    def this(message: String) = this(message, null)
    def this(cause: Throwable) = this(null, cause)
    def getMessage(): String = null
    def getCause(): Throwable = null
    def initCause(cause: Throwable): Throwable = this
    def fillInStackTrace(): Throwable = this
    def getStackTrace(): Array[StackTraceElement] = null
    def setStackTrace(stackTrace: Array[StackTraceElement]): Unit = ()
    def printStackTrace(): Unit = ()
    def printStackTrace(s: java.io.PrintStream): Unit = ()
    def printStackTrace(s: java.io.PrintWriter): Unit = ()
    override def toString(): String = null
  }

  class Thread(task: Runnable) extends Runnable {
    def this() = this(null)
    def run(): Unit = ()
    def start(): Unit = ()
    def isInterrupted(): scala.Boolean = false
    final def isAlive(): scala.Boolean = false
    final def isDaemon(): scala.Boolean = false
    final def setDaemon(on: scala.Boolean): Unit = ()
    final def getName(): String = null
    final def setName(name: String): Unit = ()
    final def setPriority(newPriority: Int): Unit = ()
    def getId(): scala.Long = 0L
    def getState(): Thread.State = null
    def getUncaughtExceptionHandler(): Thread.UncaughtExceptionHandler = null
  }

  object Thread {
    @static final val MIN_PRIORITY = 1
    @static final val NORM_PRIORITY = 5
    @static final val MAX_PRIORITY = 10
    @static def currentThread(): Thread = null
    @static def interrupted(): scala.Boolean = false
    @static def `yield`(): Unit = ()
    @static def onSpinWait(): Unit = ()
    @static def getDefaultUncaughtExceptionHandler(): UncaughtExceptionHandler = null

    trait UncaughtExceptionHandler { def uncaughtException(t: Thread, e: Throwable): Unit }

    enum State extends java.lang.Enum[State] {
      case NEW, RUNNABLE, BLOCKED, WAITING, TIMED_WAITING, TERMINATED
    }
  }
}

package java.util {
  trait List[E] extends Collection[E] {
    def size(): Int
    def isEmpty(): scala.Boolean
    def contains(o: Any): scala.Boolean
    def iterator(): Iterator[E]
    def toArray(): Array[Object]
    def toArray[T <: Object](a: Array[T]): Array[T]
    def add(e: E): scala.Boolean
    def remove(o: Any): scala.Boolean
    def containsAll(c: Collection[?]): scala.Boolean
    def addAll(c: Collection[? <: E]): scala.Boolean
    def addAll(index: Int, c: Collection[? <: E]): scala.Boolean
    def removeAll(c: Collection[?]): scala.Boolean
    def retainAll(c: Collection[?]): scala.Boolean
    def clear(): Unit
    def get(index: Int): E
    def set(index: Int, element: E): E
    def add(index: Int, element: E): Unit
    def remove(index: Int): E
    def indexOf(o: Any): Int
    def lastIndexOf(o: Any): Int
    def listIterator(): ListIterator[E]
    def listIterator(index: Int): ListIterator[E]
    def subList(fromIndex: Int, toIndex: Int): List[E]
  }

  class ArrayList[E](initialCapacity: Int) extends AbstractList[E] with List[E] with RandomAccess {
    def this() = this(10)
    def this(c: Collection[? <: E]) = this(10)
    def trimToSize(): Unit = ()
    def ensureCapacity(minCapacity: Int): Unit = ()
    override def size(): Int = 0
    override def isEmpty(): scala.Boolean = false
    override def contains(o: Any): scala.Boolean = false
    override def indexOf(o: Any): Int = 0
    override def lastIndexOf(o: Any): Int = 0
    override def toArray(): Array[Object] = null
    override def toArray[T <: Object](a: Array[T]): Array[T] = a
    def get(index: Int): E = null.asInstanceOf[E]
    override def set(index: Int, element: E): E = element
    override def add(e: E): scala.Boolean = false
    override def add(index: Int, element: E): Unit = ()
    override def remove(index: Int): E = null.asInstanceOf[E]
    override def remove(o: Any): scala.Boolean = false
    override def clear(): Unit = ()
    override def addAll(c: Collection[? <: E]): scala.Boolean = false
    override def addAll(index: Int, c: Collection[? <: E]): scala.Boolean = false
    override protected def removeRange(fromIndex: Int, toIndex: Int): Unit = ()
    override def removeAll(c: Collection[?]): scala.Boolean = false
    override def retainAll(c: Collection[?]): scala.Boolean = false
    override def listIterator(): ListIterator[E] = null
    override def listIterator(index: Int): ListIterator[E] = null
    override def iterator(): Iterator[E] = null
    override def subList(fromIndex: Int, toIndex: Int): List[E] = null
  }

  class LinkedHashMap[K, V](initialCapacity: Int, loadFactor: scala.Float, accessOrder: scala.Boolean)
      extends HashMap[K, V] {
    override def containsValue(value: Any): scala.Boolean = false
    override def get(key: Any): V = null.asInstanceOf[V]
    override def getOrDefault(key: Any, defaultValue: V): V = defaultValue
    override def clear(): Unit = ()
    protected def removeEldestEntry(eldest: Map.Entry[K, V]): scala.Boolean = false
    override def keySet(): Set[K] = null
    override def values(): Collection[V] = null
    override def entrySet(): Set[Map.Entry[K, V]] = null
    override def forEach(action: java.util.function.BiConsumer[? >: K, ? >: V]): Unit = ()
    override def replaceAll(function: java.util.function.BiFunction[? >: K, ? >: V, ? <: V]): Unit = ()
  }
  object LinkedHashMap { private[util] class Entry[K, V] }
}

package java.util.regex {
  final class Pattern extends java.io.Serializable
  object Pattern {
    @static final val UNIX_LINES = 1
    @static final val CASE_INSENSITIVE = 2
    @static final val COMMENTS = 4
    @static final val MULTILINE = 8
    @static final val LITERAL = 16
    @static final val DOTALL = 32
    @static final val UNICODE_CASE = 64
    @static final val CANON_EQ = 128
    @static final val UNICODE_CHARACTER_CLASS = 256
  }
}

package java.nio.file {
  import java.io.{BufferedWriter, InputStream, OutputStream}
  import java.nio.charset.Charset
  import java.nio.file.attribute.{BasicFileAttributes, FileAttribute, FileAttributeView, FileTime}
  import java.nio.file.attribute.{PosixFilePermission, UserPrincipal}

  abstract class FileSystem extends java.io.Closeable {
    def getPath(first: String, more: String*): Path
  }

  final class Files
  object Files {
    @static def newInputStream(path: Path, options: OpenOption*): InputStream = null
    @static def newOutputStream(path: Path, options: OpenOption*): OutputStream = null
    @static def newByteChannel(
        path: Path,
        options: java.util.Set[? <: OpenOption],
        attrs: FileAttribute[?]*
    ): java.nio.channels.SeekableByteChannel = null
    @static def newByteChannel(path: Path, options: OpenOption*): java.nio.channels.SeekableByteChannel = null
    @static def createFile(path: Path, attrs: FileAttribute[?]*): Path = null
    @static def createDirectory(dir: Path, attrs: FileAttribute[?]*): Path = null
    @static def createDirectories(dir: Path, attrs: FileAttribute[?]*): Path = null
    @static def createTempFile(dir: Path, prefix: String, suffix: String, attrs: FileAttribute[?]*): Path = null
    @static def createTempFile(prefix: String, suffix: String, attrs: FileAttribute[?]*): Path = null
    @static def createTempDirectory(dir: Path, prefix: String, attrs: FileAttribute[?]*): Path = null
    @static def createTempDirectory(prefix: String, attrs: FileAttribute[?]*): Path = null
    @static def createSymbolicLink(link: Path, target: Path, attrs: FileAttribute[?]*): Path = null
    @static def copy(source: Path, target: Path, options: CopyOption*): Path = null
    @static def copy(in: InputStream, target: Path, options: CopyOption*): scala.Long = 0L
    @static def copy(source: Path, out: OutputStream): scala.Long = 0L
    @static def move(source: Path, target: Path, options: CopyOption*): Path = null
    @static def getFileAttributeView[V <: FileAttributeView](path: Path, tpe: Class[V], options: LinkOption*): V =
      null.asInstanceOf[V]
    @static def readAttributes[A <: BasicFileAttributes](path: Path, tpe: Class[A], options: LinkOption*): A =
      null.asInstanceOf[A]
    @static def readAttributes(path: Path, attributes: String, options: LinkOption*): java.util.Map[String, Object] =
      null
    @static def setAttribute(path: Path, attribute: String, value: Object, options: LinkOption*): Path = null
    @static def getAttribute(path: Path, attribute: String, options: LinkOption*): Object = null
    @static def getPosixFilePermissions(path: Path, options: LinkOption*): java.util.Set[PosixFilePermission] = null
    @static def getOwner(path: Path, options: LinkOption*): UserPrincipal = null
    @static def isDirectory(path: Path, options: LinkOption*): scala.Boolean = false
    @static def isRegularFile(path: Path, options: LinkOption*): scala.Boolean = false
    @static def getLastModifiedTime(path: Path, options: LinkOption*): FileTime = null
    @static def exists(path: Path, options: LinkOption*): scala.Boolean = false
    @static def notExists(path: Path, options: LinkOption*): scala.Boolean = false
    @static def newBufferedWriter(path: Path, cs: Charset, options: OpenOption*): BufferedWriter = null
    @static def newBufferedWriter(path: Path, options: OpenOption*): BufferedWriter = null
    @static def write(path: Path, bytes: Array[scala.Byte], options: OpenOption*): Path = null
    @static def write(path: Path, lines: java.lang.Iterable[? <: CharSequence], cs: Charset, options: OpenOption*): Path =
      null
    @static def write(path: Path, lines: java.lang.Iterable[? <: CharSequence], options: OpenOption*): Path = null
    @static def writeString(path: Path, csq: CharSequence, options: OpenOption*): Path = null
    @static def writeString(path: Path, csq: CharSequence, cs: Charset, options: OpenOption*): Path = null
    @static def walk(start: Path, maxDepth: Int, options: FileVisitOption*): java.util.stream.Stream[Path] = null
    @static def walk(start: Path, options: FileVisitOption*): java.util.stream.Stream[Path] = null
    @static def find(
        start: Path,
        maxDepth: Int,
        matcher: java.util.function.BiPredicate[Path, BasicFileAttributes],
        options: FileVisitOption*
    ): java.util.stream.Stream[Path] = null
  }
}

package java.io {
  class BufferedInputStream(in: InputStream, size: Int) extends FilterInputStream(in) {
    def this(in: InputStream) = this(in, 8192)
    protected var buf: Array[Byte] = null
    protected var count: Int = 0
    protected var pos: Int = 0
    protected var markpos: Int = -1
    protected var marklimit: Int = 0
  }
}

package scala.scalanative.javalibintf {
  final class PointerBuffer
  object PointerBuffer {
    @static def wrapPointerByte(ptr: Object, size: Int): java.nio.ByteBuffer = null
    @static def hasPointer(buffer: java.nio.Buffer): scala.Boolean = false
    @static def pointer(buffer: java.nio.Buffer): Object = null
  }

  final class String
  object String {
    @static def fromByteBuffer(data: java.nio.ByteBuffer, encoding: java.nio.charset.Charset): java.lang.String = null
  }
}
