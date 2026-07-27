use concord::codec::{AnyMedia, MediaType};

fn require_exact_media<M: MediaType>() {}

fn main() {
    require_exact_media::<AnyMedia>();
}
