//> using options -Yexplicit-nulls --fresh -classpath libs/javalib-tasty-0.5.12.jar
object SafeEq:
  def isNull(x: String | Null): Boolean = x eq null
  def notNull(x: String | Null): Boolean = x ne null
  def leftNull(x: Null | String): Boolean = x eq null
  def same(x: String | Null, y: AnyRef): Boolean = x eq y
  def java(x: String): Boolean = x.substring(1) eq null
  def nullFirst(x: String | Null): Boolean = null eq x
