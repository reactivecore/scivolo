import java.util.ServiceLoader

object Ni26services {
  def providers(): Boolean = ServiceLoader.load(classOf[Runnable]).iterator().hasNext

  def main(args: Array[String]): Unit = {
    println(providers())
    println(ServiceLoader.loadInstalled(classOf[Runnable]).iterator().hasNext)
  }
}
