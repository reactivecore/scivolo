class Counter:
  type Item = String

class Wrapper(c: Counter):
  export c.Item
