package java.util.concurrent

/** `helpAsyncBlocker`, which does nothing on threads other than workers. */
abstract class ForkJoinPool extends AbstractExecutorService

object ForkJoinPool {
  trait ManagedBlocker

  final class WorkQueue {
    private[concurrent] def helpAsyncBlocker(blocker: ManagedBlocker): Unit = ???
  }

  def commonPool(): ForkJoinPool = ???
  private[concurrent] def commonQueue(): WorkQueue = ???

  private[concurrent] def helpAsyncBlocker(e: Executor, blocker: ManagedBlocker): Unit =
    Thread.currentThread() match {
      case wt: ForkJoinWorkerThread =>
        val w =
          if (wt.pool eq e) wt.workQueue
          else if (e eq commonPool()) commonQueue()
          else null
        if (w != null) w.helpAsyncBlocker(blocker)
      case _ => ()
    }
}

abstract class ForkJoinWorkerThread extends Thread {
  private[concurrent] def pool: ForkJoinPool = ???
  private[concurrent] def workQueue: ForkJoinPool.WorkQueue = ???
}
