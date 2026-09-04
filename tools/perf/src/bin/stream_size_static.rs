#![forbid(unsafe_code)]

use std::hint::black_box;

struct Driver {
    remaining: usize,
}

impl Driver {
    fn next(&mut self) -> Option<usize> {
        let next = self.remaining.checked_sub(1)?;
        self.remaining = next;
        Some(next)
    }
}

fn main() {
    let mut driver = Driver { remaining: 1_000 };
    let mut sum = 0_usize;
    while let Some(value) = driver.next() {
        sum = sum.wrapping_add(black_box(value));
    }
    println!("{}", black_box(sum));
}
