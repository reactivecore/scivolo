inline def twice(inline thunk: =>Unit): Unit =
  thunk
  thunk

inline def pipe(inline t: =>Unit, inline f: (=>Unit) => Unit): Unit = f(t)

@main def Lw26byname =
  pipe((), twice)
  pipe(println("foo"), twice)
