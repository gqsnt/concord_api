// Head-only absence is a library-sealed relationship implemented only for
// `Option<T>`; an application cannot introduce its own missing-outcome rule.
use concord_core::contracts::OptionalOutcome;

enum Lookup {
    Found(u32),
    Missing,
}

impl OptionalOutcome for Lookup {
    type Present = u32;
}

fn main() {
    let _ = (Lookup::Found(1), Lookup::Missing);
}
