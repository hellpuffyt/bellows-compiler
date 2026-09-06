// FizzBuzz 1..15 — exercises print of strings and ints, `&&`, nested if/else.
// expect: 1
// expect: 2
// expect: Fizz
// expect: 4
// expect: Buzz
// expect: Fizz
// expect: 7
// expect: 8
// expect: Fizz
// expect: Buzz
// expect: 11
// expect: Fizz
// expect: 13
// expect: 14
// expect: FizzBuzz

fn main() {
    let mut i = 1;
    while i <= 15 {
        let by3 = i % 3 == 0;
        let by5 = i % 5 == 0;
        if by3 && by5 {
            print("FizzBuzz");
        } else if by3 {
            print("Fizz");
        } else if by5 {
            print("Buzz");
        } else {
            print(i);
        }
        i = i + 1;
    }
}
