// Benchmark workload: fib(30) recursively plus a 10-million-iteration
// arithmetic loop. `tools/bench.sh` times this compiled vs interpreted.
// expect: 832040
// expect: 4999985000000

fn fib(n: i64) -> i64 {
    if n < 2 {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

fn main() {
    print(fib(30));
    let mut i = 0;
    let mut acc = 0;
    while i < 10000000 {
        acc = acc + (i * 5 + 1) % 1000000 / 1 + 0;
        i = i + 1;
    }
    print(acc);
}
