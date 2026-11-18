import java.util.concurrent.{ConcurrentLinkedQueue, CountDownLatch, CyclicBarrier}
import java.util.concurrent.atomic.AtomicInteger
import scala.annotation.static

class Counted(counter: AtomicInteger, gate: CountDownLatch):
  lazy val value: String =
    counter.incrementAndGet()
    gate.await()
    "computed"
  lazy val nothing: String =
    counter.incrementAndGet()
    gate.await()
    null

class Failing(counter: AtomicInteger):
  lazy val value: Int =
    if counter.incrementAndGet() < 3 then throw new IllegalStateException("try " + counter.get)
    counter.get

class Early:
  lazy val value: String = "handles first"

object Early:
  @static val first: String = (new Early).value

trait Mixed:
  lazy val mixed: String = "mixed"

object Single extends Mixed:
  lazy val own: Int = 1

object Lazyhandles:
  val Threads = 8
  val Rounds = 20

  def concurrent(read: Counted => Any): (Int, Set[Any]) =
    val counter = new AtomicInteger
    val gate = new CountDownLatch(1)
    val c = new Counted(counter, gate)
    val barrier = new CyclicBarrier(Threads + 1)
    val results = new ConcurrentLinkedQueue[Any]
    val threads = (1 to Threads).map: _ =>
      val t = new Thread(() =>
        barrier.await()
        results.add(Option(read(c)))
        ()
      )
      t.start()
      t
    barrier.await()
    Thread.sleep(5)
    gate.countDown()
    threads.foreach(_.join())
    (counter.get, results.toArray.toSet)

  def main(args: Array[String]): Unit =
    for _ <- 1 to Rounds do
      assert(concurrent(_.value) == (1, Set(Some("computed"))))
      assert(concurrent(_.nothing) == (1, Set(None)))
    println("one initialization")
    val failing = new Failing(new AtomicInteger)
    for _ <- 1 to 3 do
      try println(failing.value)
      catch case e: IllegalStateException => println(e.getMessage)
    println(failing.value)
    println(Early.first)
    println(Single.mixed + Single.own)
