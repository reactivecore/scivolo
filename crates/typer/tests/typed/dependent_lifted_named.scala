trait Box:
  type T
  def v: T

object Main:
  def make(): Box { type T = Int } = new Box { type T = Int; def v = 1 }
  def get(flag: Boolean, b: Box): b.T = b.v
  val n: Int = get(b = make(), flag = true)
