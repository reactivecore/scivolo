fn fib(n: i32) -> i32 { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } }
fn main() {
    println!("fib(30) = {}", fib(30));
    let mut seed: i64 = 42;
    let mut v: Vec<i32> = (0..100000).map(|_| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((seed as u64) >> 33) as i32
    }).collect();
    v.sort();
    println!("sorted: n={} min={} max={} mid={}", v.len(), v[0], v[v.len()-1], v[50000]);
}
