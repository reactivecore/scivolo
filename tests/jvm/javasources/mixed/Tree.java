package mixed;

import java.util.List;

/** A generic class with an inner class that uses its type variable. */
public class Tree<T> {
    public class Node {
        public final T value;
        public Node(T value) { this.value = value; }
        public Tree<T> tree() { return Tree.this; }
    }

    public Node root(T value) { return new Node(value); }

    public Tree<T>.Node leaf(T value) { return new Node(value); }

    public int[] sizes(List<String> names)[] { return new int[][] {{names.size()}}; }
}
