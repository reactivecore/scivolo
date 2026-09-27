//> using scala 3.9.0

import java.util.concurrent.{CountDownLatch, Executors, TimeUnit}
import java.util.concurrent.atomic.AtomicInteger
import scala.concurrent.{Await, Future}
import scala.concurrent.ExecutionContext.Implicits.global
import scala.concurrent.duration.*

class Counter:
  private var n = 0
  def inc(): Unit = synchronized { n += 1 }
  def get: Int = synchronized(n)

class Mailbox:
  private var item: Option[Int] = None
  def put(i: Int): Unit = synchronized {
    while item.isDefined do wait()
    item = Some(i)
    notifyAll()
  }
  def take(): Int = synchronized {
    while item.isEmpty do wait()
    val i = item.get
    item = None
    notifyAll()
    i
  }

@main def threads(): Unit =
  val counter = Counter()
  val workers = (1 to 4).map(_ => Thread(() => for _ <- 1 to 10000 do counter.inc()))
  workers.foreach(_.start())
  workers.foreach(_.join())
  println(s"counter ${counter.get}")

  val box = Mailbox()
  val producer = Thread(() => for i <- 1 to 100 do box.put(i))
  producer.start()
  println(s"received ${(1 to 100).map(_ => box.take()).sum}")
  producer.join()

  val sleeper = Thread(() =>
    try
      Thread.sleep(60000)
      println("slept")
    catch case _: InterruptedException => println("interrupted")
  )
  sleeper.start()
  Thread.sleep(20)
  sleeper.interrupt()
  sleeper.join()

  val latch = CountDownLatch(3)
  val hits = AtomicInteger()
  val pool = Executors.newFixedThreadPool(3)
  for _ <- 1 to 3 do pool.execute(() => { hits.incrementAndGet(); latch.countDown() })
  println(s"latch ${latch.await(5, TimeUnit.SECONDS)} hits ${hits.get}")
  pool.shutdown()
  println(s"pool ${pool.awaitTermination(5, TimeUnit.SECONDS)}")

  val futures = Future.sequence((1 to 10).map(i => Future(i * i)))
  println(s"futures ${Await.result(futures, 10.seconds).sum}")
