use std::time::Instant;

fn main() {
    let a = image::open("examples/before.png").unwrap().to_rgba8();
    let b = image::open("examples/after.png").unwrap().to_rgba8();

    let start = Instant::now();
    for _ in 0..50 {
        let _ = shotdiff::build_diff(&a, &b, 16); // Assuming build_diff is not pub...
    }
    println!("Time: {:?}", start.elapsed());
}
