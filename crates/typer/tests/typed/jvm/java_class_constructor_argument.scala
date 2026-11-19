import java.util.ArrayList

trait Bind[Z[_]]

object Bind:
  val arrayListBind: Bind[ArrayList] = new Bind[ArrayList] {}
  val listBind = new Bind[java.util.List] {}
