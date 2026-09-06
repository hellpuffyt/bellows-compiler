// Trial-division primes with nested loops, early return and `%`.
// expect: 2
// expect: 3
// expect: 5
// expect: 7
// expect: 11
// expect: 13
// expect: 17
// expect: 19
// expect: 23
// expect: 29
// expect: count 25

fn is_prime(n: i64) -> i64 {
    if n < 2 {
        return 0;
    }
    let mut d = 2;
    while d * d <= n {
        if n % d == 0 {
            return 0;
        }
        d = d + 1;
    }
    return 1;
}

fn main() {
    let mut n = 0;
    let mut count = 0;
    while n < 100 {
        if is_prime(n) == 1 {
            if n < 30 {
                print(n);
            }
            count = count + 1;
        }
        n = n + 1;
    }
    print("count 25");
    if count != 25 {
        print("WRONG");
    }
}
