pub mod recipes;

pub fn slow_function() {
    let batch = 10_000;
    let mut m:Vec<u64> = vec![];
    for i in 0..batch {
        for j in 0..batch {
            let k = i * j;
            m.push(k * k);
        }
    }
}
