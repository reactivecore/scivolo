// The JDK's declarations of classes javalib or nativelib also defines that every compile, nativelib's
// too, sees over theirs (ADR 0082), as JdkDeclarations.scala's: a type parameter is `<: Object` where
// an array of it erases to Java's `Object[]`; a class Scala Native renames is declared under its name.
import scala.annotation.static

package java.lang {
  abstract class ClassLoader { def loadClass(name: String): Class[?] = null }

  class Runtime private () {
    def totalMemory(): scala.Long = 0L
    def maxMemory(): scala.Long = 0L
    def freeMemory(): scala.Long = 0L
  }

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
  class AccessibleObject {
    def setAccessible(flag: scala.Boolean): Unit = ()
    final def trySetAccessible(): scala.Boolean = false
    def isAccessible(): scala.Boolean = false
    final def canAccess(obj: Object): scala.Boolean = false
  }

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

  trait Enumeration[E] { def asIterator(): Iterator[E] = null }
}

package java.util {
  class Properties extends Hashtable[Object, Object] {
    override def size(): Int = 0
    override def isEmpty(): scala.Boolean = true
    override def keys(): Enumeration[Object] = null
    override def elements(): Enumeration[Object] = null
    override def contains(value: Object): scala.Boolean = false
    override def containsValue(value: Object): scala.Boolean = false
    override def containsKey(key: Object): scala.Boolean = false
    override def get(key: Object): Object = null
    override def put(key: Object, value: Object): Object = null
    override def remove(key: Object): Object = null
    override def putAll(t: Map[? <: Object, ? <: Object]): Unit = ()
    override def clear(): Unit = ()
    override def toString(): String = null
    override def keySet(): Set[Object] = null
    override def values(): Collection[Object] = null
    override def entrySet(): Set[Map.Entry[Object, Object]] = null
    override def equals(o: Any): scala.Boolean = false
    override def hashCode(): Int = 0
    override def getOrDefault(key: Object, defaultValue: Object): Object = null
    override def forEach(action: function.BiConsumer[? >: Object, ? >: Object]): Unit = ()
    override def replaceAll(f: function.BiFunction[? >: Object, ? >: Object, ? <: Object]): Unit = ()
    override def putIfAbsent(key: Object, value: Object): Object = null
    override def remove(key: Object, value: Object): scala.Boolean = false
    override def replace(key: Object, oldValue: Object, newValue: Object): scala.Boolean = false
    override def replace(key: Object, value: Object): Object = null
    override def computeIfAbsent(key: Object, mappingFunction: function.Function[? >: Object, ? <: Object]): Object = null
    override def computeIfPresent(key: Object, remappingFunction: function.BiFunction[? >: Object, ? >: Object, ? <: Object]): Object =
      null
    override def compute(key: Object, remappingFunction: function.BiFunction[? >: Object, ? >: Object, ? <: Object]): Object = null
    override def merge(key: Object, value: Object, remappingFunction: function.BiFunction[? >: Object, ? >: Object, ? <: Object]): Object =
      null
    override def clone(): Object = null
  }
}

package java.util.stream {
  trait IntStream extends BaseStream[Integer, IntStream] {
    def sequential(): IntStream
    def parallel(): IntStream
    def iterator(): java.util.PrimitiveIterator.OfInt
  }
  trait LongStream extends BaseStream[java.lang.Long, LongStream] {
    def sequential(): LongStream
    def parallel(): LongStream
    def iterator(): java.util.PrimitiveIterator.OfLong
  }
  trait DoubleStream extends BaseStream[java.lang.Double, DoubleStream] {
    def sequential(): DoubleStream
    def parallel(): DoubleStream
    def iterator(): java.util.PrimitiveIterator.OfDouble
  }
}

package java.util.concurrent {
  import java.util.function.Function as JFunction

  trait ExecutorService extends Executor {
    def submit[T](task: Callable[T]): Future[T]
    def submit[T](task: Runnable, result: T): Future[T]
    def submit(task: Runnable): Future[?]
    def invokeAll[T](tasks: java.util.Collection[? <: Callable[T]]): java.util.List[Future[T]]
    def invokeAll[T](tasks: java.util.Collection[? <: Callable[T]], timeout: Long, unit: TimeUnit): java.util.List[Future[T]]
    def invokeAny[T](tasks: java.util.Collection[? <: Callable[T]]): T
    def invokeAny[T](tasks: java.util.Collection[? <: Callable[T]], timeout: Long, unit: TimeUnit): T
  }
  trait CompletionStage[T <: Object] {
    def applyToEither[U <: Object](other: CompletionStage[? <: T], fn: JFunction[? >: T, U]): CompletionStage[U]
    def applyToEitherAsync[U <: Object](other: CompletionStage[? <: T], fn: JFunction[? >: T, U]): CompletionStage[U]
    def applyToEitherAsync[U <: Object](other: CompletionStage[? <: T], fn: JFunction[? >: T, U], executor: Executor):
      CompletionStage[U]
  }
  abstract class CompletableFuture[T <: Object] extends Future[T] with CompletionStage[T] {
    override def applyToEither[U <: Object](other: CompletionStage[? <: T], fn: JFunction[? >: T, U]): CompletableFuture[U] =
      null
    override def applyToEitherAsync[U <: Object](other: CompletionStage[? <: T], fn: JFunction[? >: T, U]):
      CompletableFuture[U] = null
    override def applyToEitherAsync[U <: Object](other: CompletionStage[? <: T], fn: JFunction[? >: T, U], executor: Executor):
      CompletableFuture[U] = null
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
