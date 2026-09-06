// Recursive and iterative Fibonacci — the classic compiler smoke test.
// expect: 6765
// expect: 6765
// expect: true

fn fib(n: i64) -> i64 {
    if n < 2 {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

fn fib_iter(n: i64) -> i64 {
    let mut a = 0;
    let mut b = 1;
    let mut i = 0;
    while i < n {
        let t = a + b;
        a = b;
        b = t;
        i = i + 1;
    }
    return a;
}

fn main() {
    print(fib(20));
    print(fib_iter(20));
    print(fib(20) == fib_iter(20));
}
