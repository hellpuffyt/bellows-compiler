// Short-circuit evaluation, negatives, precedence, booleans, nested calls.
// expect: true
// expect: false
// expect: -7
// expect: 14
// expect: 1
// expect: true
// expect: safe
// expect: 8

fn noisy(v: bool) -> bool {
    print("side effect");
    return v;
}

fn abs(x: i64) -> i64 {
    if x < 0 {
        return -x;
    }
    return x;
}

fn main() {
    print(1 < 2 && 2 < 3);
    print(1 < 2 && 2 > 3);
    print(-7);
    print(2 + 3 * 4);
    print((2 + 3) * 4 / 20);
    print(!(1 == 2) || noisy(true));   // noisy is never called
    let d = 0;
    if d != 0 && 10 / d > 1 {
        print("unreachable");
    } else {
        print("safe");
    }
    print(abs(-3) + abs(5));
}
