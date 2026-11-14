#![allow(unreachable_code, unused_braces, clippy::needless_return)]
::concord::api! {
    client Returns {
        base "https://returns.test";
        read { return ::concord::Json; };
        problem u32 read { return ::concord::Json; };
        POST post() { idempotent { return true; }; body { return Ok(::concord::json(1)); }; }
        GET page(start?: u32) -> Vec<u32> {
            query { start; }
            paginate { return ::concord::by_offset(start); };
        }
        GET arm() -> u32 { status { 404 => { return Ok(17); } } }
    }
}
fn main() { let _ = Returns::new().page().pages(); }
