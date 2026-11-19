import java.util.ArrayList

trait Bind[Z[_]]
trait Two[M[_, _]]

object Bind:
  val arrayListBind: Bind[ArrayList] = new Bind[ArrayList] {}
  val listBind = new Bind[java.util.List] {}
  val mapTwo = new Two[java.util.concurrent.ConcurrentHashMap] {}
