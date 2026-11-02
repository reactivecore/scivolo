//> using options -Yexplicit-nulls --fresh -classpath libs/javalib-tasty-0.5.12.jar
object JavaMembers:
  val sub = "abc".substring(1)
  val num = String.valueOf(1)
  val sb = new java.lang.StringBuilder().append("a")
  val arr = java.util.Arrays.copyOf(Array(1, 2), 1)
