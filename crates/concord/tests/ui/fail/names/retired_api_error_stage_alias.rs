use concord::error::ApiError;

fn retired_stage_accessor(error: &ApiError) {
    let _ = error.stage();
}

fn main() {}
