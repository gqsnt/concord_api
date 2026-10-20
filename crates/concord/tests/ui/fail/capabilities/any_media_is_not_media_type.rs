// §9.2: the retired type-level media API (`AnyMedia`, `MediaType`) stays
// absent; each representation owns one exact static Content-Type/Accept.
use concord::codec::{AnyMedia, MediaType};

fn require_exact_media<M: MediaType>() {}

fn main() {
    require_exact_media::<AnyMedia>();
}
