// The JDK classes and annotations Scala Native's library sources use and javalib does not define,
// and nativelib's Java class, for `just lib-compile`: only the members those sources use, Java's static members `@static`.
import scala.annotation.static

package java.lang {
  final class Deprecated extends scala.annotation.StaticAnnotation
  final class FunctionalInterface extends scala.annotation.StaticAnnotation
  final class SuppressWarnings(value: Array[String]) extends scala.annotation.StaticAnnotation
}

package java.lang.invoke {
  final class MethodHandles
  abstract class VarHandle
}

package java.lang.reflect {
  trait AnnotatedElement
  final class Field
  final class Method
}

package java.time {
  final class Duration private () {
    def getSeconds(): Long = 0L
    def getNano(): Int = 0
  }

  final class Instant private () {
    def getEpochSecond: Long = 0L
    def getNano: Int = 0
    def toEpochMilli(): Long = 0L
  }

  object Instant {
    @static val MIN: Instant = new Instant()
    @static val MAX: Instant = new Instant()
    @static def ofEpochMilli(epochMilli: Long): Instant = MIN
    @static def ofEpochSecond(epochSecond: Long): Instant = MIN
    @static def ofEpochSecond(epochSecond: Long, nanoAdjustment: Long): Instant = MIN
  }
}

package java.time.temporal {
  trait TemporalUnit

  enum ChronoUnit extends java.lang.Enum[ChronoUnit] with TemporalUnit {
    case NANOS, MICROS, MILLIS, SECONDS, MINUTES, HOURS, HALF_DAYS, DAYS, WEEKS, MONTHS, YEARS,
      DECADES, CENTURIES, MILLENNIA, ERAS, FOREVER
  }
}

package java.security {
  class SecureRandom extends java.util.Random {
    override def nextBytes(bytes: Array[Byte]): Unit = ()
  }
  trait PrivilegedAction[T] { def run(): T }
  trait PrivilegedExceptionAction[T] { def run(): T }
  final class CodeSigner
}

package java.security.cert {
  abstract class Certificate
  abstract class X509Certificate extends Certificate
  class CertificateException extends java.security.GeneralSecurityException
  class CertificateFactory
}

package java.text {
  class DecimalFormatSymbols {
    def getZeroDigit(): Char = '0'
    def getDecimalSeparator(): Char = '.'
    def getGroupingSeparator(): Char = ','
  }

  object DecimalFormatSymbols {
    @static def getInstance(locale: java.util.Locale): DecimalFormatSymbols = new DecimalFormatSymbols()
  }

  abstract class NumberFormat

  object NumberFormat {
    @static def getNumberInstance(locale: java.util.Locale): NumberFormat = new DecimalFormat()
  }

  class DecimalFormat extends NumberFormat {
    def getGroupingSize(): Int = 3
  }
}

package javax.security.auth {
  final class Subject {
    def getPrincipals(): java.util.Set[java.security.Principal] = null
  }
}

package javax.security.auth.x500 {
  final class X500Principal
}

package java.io {
  class ObjectInputStream extends InputStream {
    def read(): Int = -1
    def defaultReadObject(): Unit = ()
    final def readObject(): Object = null
  }

  object ObjectOutputStream {
    abstract class PutField {
      def put(name: String, value: Boolean): Unit
      def put(name: String, value: Byte): Unit
      def put(name: String, value: Char): Unit
      def put(name: String, value: Short): Unit
      def put(name: String, value: Int): Unit
      def put(name: String, value: Long): Unit
      def put(name: String, value: Float): Unit
      def put(name: String, value: Double): Unit
      def put(name: String, value: Object): Unit
    }
  }

  class ObjectOutputStream extends OutputStream {
    def write(b: Int): Unit = ()
    def defaultWriteObject(): Unit = ()
    def putFields(): ObjectOutputStream.PutField = null
    def writeFields(): Unit = ()
    final def writeObject(obj: Object): Unit = ()
  }
}

package java.nio.channels {
  trait AsynchronousChannel extends Channel
  abstract class AsynchronousFileChannel extends AsynchronousChannel
  abstract class DatagramChannel
}

package java.nio.file {
  abstract class FileStore
}

package java.util {
  final class Locale(language: String) {
    def getLanguage(): String = language
  }

  object Locale {
    @static val ROOT: Locale = new Locale("")
  }
}

package java.net {
  final class URL(spec: String)
}

package scala.scalanative.runtime {
  abstract class UnsupportedFeature

  object UnsupportedFeature {
    @static def threads(): Unit = ()
    @static def virtualThreads(): Unit = ()
    @static def continuations(): Unit = ()
  }
}
