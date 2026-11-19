import scala.language.reflectiveCalls

trait Foo extends reflect.Selectable

def readVar: Int =
  val x = new Foo { var i: Int = 0 }
  x.i

def assignVar: Int =
  val x = new Foo { var i: Int = 0 }
  x.i = 1
  x.i

def readVal: Int =
  val x = new Foo { val i: Int = 2 }
  x.i

def readDef: Int =
  val x = new Foo { def i: Int = 3 }
  x.i

@main def Test(): Unit =
  println(readVar)
  println(assignVar)
  println(readVal)
  println(readDef)
