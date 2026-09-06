// Longest Collatz chain under 10000, with a helper and `else if`.
// expect: 6171
// expect: 261

fn steps(n: i64) -> i64 {
    let mut x = n;
    let mut s = 0;
    while x != 1 {
        if x % 2 == 0 {
            x = x / 2;
        } else {
            x = 3 * x + 1;
        }
        s = s + 1;
    }
    return s;
}

fn main() {
    let mut best = 1;
    let mut best_len = 0;
    let mut n = 1;
    while n < 10000 {
        let s = steps(n);
        if s > best_len {
            best_len = s;
            best = n;
        } else if s == best_len {
            // ties keep the smaller n
        }
        n = n + 1;
    }
    print(best);
    print(best_len);
}
