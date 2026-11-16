package mixed;

public class Hidden {
    public final String made;

    private Hidden() { made = "none"; }
    public Hidden(int a) { made = "int " + a; }
    private Hidden(String s) { made = "string " + s; }
    public Hidden(Object o) { made = "object " + o; }
}
