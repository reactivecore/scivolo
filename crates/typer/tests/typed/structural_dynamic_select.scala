import scala.language.dynamics

object StructuralDynamicSelect {
  trait Sel extends Dynamic
  extension (s: Sel) def selectDynamic(name: String): Any = name
  def sel2: Sel { def foo: String } = (new Sel {}).asInstanceOf[Sel { def foo: String }]
  def foo2: String = sel2.foo
}
