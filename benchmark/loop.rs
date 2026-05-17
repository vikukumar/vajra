fn main() {
    let mut sum: i64 = 0;
    let mut i: i64 = 0;
    while i < 1000000000 {
        sum += i;
        i += 1;
    }
    println!("{}", sum);
}
