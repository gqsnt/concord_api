#![forbid(unsafe_code)]

fn main() -> Result<(), concord::ClientBuildError> {
    concord_examples::request_response::records::construct()?;
    println!("constructed buffered and streaming record requests; local chunk proofs run in tests");
    Ok(())
}
