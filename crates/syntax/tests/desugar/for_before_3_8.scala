//> using options -source:3.7
def m = for
  x <- xs
  y = x + 1
  if y > 2
  (a, b) <- ps
yield a
