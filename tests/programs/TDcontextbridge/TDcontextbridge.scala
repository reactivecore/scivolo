//> using scala 3.9.0
// A method with a flattened context function result implementing an abstract one.

class TDTx { def name = "tx" }

trait TDOps {
  def f1(x: Int): TDTx ?=> Int
}

object TDOpsObj extends TDOps {
  def f1(x: Int): TDTx ?=> Int = summon[TDTx].name.length + x
}

object TDcontextbridge {
  def main(args: Array[String]): Unit = {
    given TDTx = new TDTx
    val ops: TDOps = TDOpsObj
    println(ops.f1(2))
    println(TDOpsObj.f1(3))
  }
}
