// The JDK's declarations of classes javalib or nativelib also defines that every compile, nativelib's
// too, sees over theirs (ADR 0082), as JdkDeclarations.scala's: a type parameter is `<: Object` where
// an array of it erases to Java's `Object[]`; a class Scala Native renames is declared under its name.
import scala.annotation.static

package java.lang {
  final class System
  object System { @static def console(): java.io.Console = null }
}

package java.lang.ref {
  abstract class Reference[T]
  class SoftReference[T] extends Reference[T] { def get(): T = null.asInstanceOf[T] }
  class WeakReference[T] extends Reference[T]
  class PhantomReference[T] extends Reference[T] { def get(): T = null.asInstanceOf[T] }
  class ReferenceQueue[T]
}

package java.lang.reflect {
  final class Array
  object Array {
    @static def newInstance(componentType: Class[?], length: Int): Object = null
    @static def newInstance(componentType: Class[?], dimensions: Int*): Object = null
  }
}

package java.nio.file {
  trait Path { def toRealPath(options: LinkOption*): Path }
}

package java.util {
  final class Arrays
  object Arrays {
    @static def sort(a: Array[Int]): Unit = ()
    @static def sort(a: Array[Int], fromIndex: Int, toIndex: Int): Unit = ()
    @static def sort(a: Array[scala.Long]): Unit = ()
    @static def sort(a: Array[scala.Long], fromIndex: Int, toIndex: Int): Unit = ()
    @static def sort(a: Array[scala.Short]): Unit = ()
    @static def sort(a: Array[scala.Short], fromIndex: Int, toIndex: Int): Unit = ()
    @static def sort(a: Array[Char]): Unit = ()
    @static def sort(a: Array[Char], fromIndex: Int, toIndex: Int): Unit = ()
    @static def sort(a: Array[scala.Byte]): Unit = ()
    @static def sort(a: Array[scala.Byte], fromIndex: Int, toIndex: Int): Unit = ()
    @static def sort(a: Array[scala.Float]): Unit = ()
    @static def sort(a: Array[scala.Float], fromIndex: Int, toIndex: Int): Unit = ()
    @static def sort(a: Array[scala.Double]): Unit = ()
    @static def sort(a: Array[scala.Double], fromIndex: Int, toIndex: Int): Unit = ()
    @static def sort(a: Array[Object]): Unit = ()
    @static def sort(a: Array[Object], fromIndex: Int, toIndex: Int): Unit = ()
    @static def sort[T <: Object](a: Array[T], c: Comparator[? >: T]): Unit = ()
    @static def sort[T <: Object](a: Array[T], fromIndex: Int, toIndex: Int, c: Comparator[? >: T]): Unit = ()
    @static def copyOf[T <: Object](original: Array[T], newLength: Int): Array[T] = original
    @static def copyOf[T <: Object, U <: Object](original: Array[U], newLength: Int, newType: Class[? <: Array[T]]): Array[T] =
      null
    @static def copyOf(original: Array[scala.Byte], newLength: Int): Array[scala.Byte] = original
    @static def copyOf(original: Array[scala.Short], newLength: Int): Array[scala.Short] = original
    @static def copyOf(original: Array[Int], newLength: Int): Array[Int] = original
    @static def copyOf(original: Array[scala.Long], newLength: Int): Array[scala.Long] = original
    @static def copyOf(original: Array[Char], newLength: Int): Array[Char] = original
    @static def copyOf(original: Array[scala.Float], newLength: Int): Array[scala.Float] = original
    @static def copyOf(original: Array[scala.Double], newLength: Int): Array[scala.Double] = original
    @static def copyOf(original: Array[scala.Boolean], newLength: Int): Array[scala.Boolean] = original
  }
}

package java.util.concurrent {
  trait ExecutorService extends Executor {
    def submit[T](task: Callable[T]): Future[T]
    def submit[T](task: Runnable, result: T): Future[T]
    def submit(task: Runnable): Future[?]
    def invokeAll[T](tasks: java.util.Collection[? <: Callable[T]]): java.util.List[Future[T]]
    def invokeAll[T](tasks: java.util.Collection[? <: Callable[T]], timeout: Long, unit: TimeUnit): java.util.List[Future[T]]
    def invokeAny[T](tasks: java.util.Collection[? <: Callable[T]]): T
    def invokeAny[T](tasks: java.util.Collection[? <: Callable[T]], timeout: Long, unit: TimeUnit): T
  }
}

package scala.scalanative.runtime {
  final class _Class[T] {
    def getField(name: String): java.lang.reflect.Field = null
    def getMethod(name: String, parameterTypes: Class[?]*): java.lang.reflect.Method = null
  }
  object _Class {
    @static def forName(className: String): Class[?] = null
    @static def forName(name: String, initialize: scala.Boolean, loader: ClassLoader): Class[?] = null
    @static def forName(module: Module, name: String): Class[?] = null
  }
}
