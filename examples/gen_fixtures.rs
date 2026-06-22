// Throwaway: generate two test screenshots that differ in a region.
use image::{Rgba, RgbaImage};
fn main() {
    let (w, h) = (240u32, 360u32);
    let mut a = RgbaImage::from_pixel(w, h, Rgba([245, 245, 245, 255]));
    let mut b = RgbaImage::from_pixel(w, h, Rgba([245, 245, 245, 255]));
    // common header bar (identical)
    for y in 0..48 {
        for x in 0..w {
            a.put_pixel(x, y, Rgba([30, 90, 200, 255]));
            b.put_pixel(x, y, Rgba([30, 90, 200, 255]));
        }
    }
    // a button that moved/changed colour between the two
    for y in 120..170 {
        for x in 40..200 {
            a.put_pixel(x, y, Rgba([200, 60, 60, 255])); // before: red
            b.put_pixel(x, y, Rgba([60, 180, 90, 255])); // after: green
        }
    }
    // identical footer text block
    for y in 300..330 {
        for x in 20..220 {
            a.put_pixel(x, y, Rgba([180, 180, 180, 255]));
            b.put_pixel(x, y, Rgba([180, 180, 180, 255]));
        }
    }
    a.save("examples/before.png").unwrap();
    b.save("examples/after.png").unwrap();

    // A smaller variant to exercise the size-mismatch path.
    let small = image::imageops::resize(&b, 200, 300, image::imageops::FilterType::Triangle);
    small.save("examples/after_small.png").unwrap();
    println!("wrote examples/before.png, after.png, after_small.png");
}
