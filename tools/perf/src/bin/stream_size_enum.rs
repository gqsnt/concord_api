#![forbid(unsafe_code)]

use std::hint::black_box;

enum Driver {
    Counter { remaining: usize },
}

impl Driver {
    fn next(&mut self) -> Option<usize> {
        match self {
            Self::Counter { remaining } => {
                let next = remaining.checked_sub(1)?;
                *remaining = next;
                Some(next)
            }
        }
    }
}

fn main() {
    let mut driver = Driver::Counter { remaining: 1_000 };
    let mut sum = 0_usize;
    while let Some(value) = driver.next() {
        sum = sum.wrapping_add(black_box(value));
    }
    println!("{}", black_box(sum));
}
