package mixed;

/** A record with an interface, a varargs component, a compact constructor and statics. */
public record Pair(String name, int... counts) implements Comparable<Pair> {
    public static final String BLOCK = """
        two\tlines\101 \s
          indented\
         joined
        """;
    static int made = 0;

    public Pair {
        made++;
    }

    public static Pair of(String name) { return new Pair(name, 1, 2); }

    public static int made() { return made; }

    public int total() {
        int t = 0;
        for (int c : counts) t += c;
        return t;
    }

    public int compareTo(Pair other) { return Integer.compare(total(), other.total()); }
}
