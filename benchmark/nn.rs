fn main() {
    let mut sum: i64 = 0;
    let layers: i64 = 1000000;
    let nodes: i64 = 10000;
    
    let mut l: i64 = 0;
    while l < layers {
        let t1 = l * 17;
        let mut n: i64 = 0;
        while n < nodes {
            let t2 = n * 31;
            let val = t1 + t2;
            
            let div = val / 10;
            let term = div * 10;
            let weight = val - term;
            
            let val2 = l + n;
            let div2 = val2 / 5;
            let term2 = div2 * 5;
            let bias = val2 - term2;
            
            let t3 = weight * n;
            let act = t3 + bias;
            if act > 5 {
                sum += act;
            }
            n += 1;
        }
        l += 1;
    }
    println!("{}", sum);
}
