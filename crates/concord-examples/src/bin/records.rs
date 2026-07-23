#![forbid(unsafe_code)]

fn main() -> Result<(), concord::ClientBuildError> {
    concord_examples::request_response::records::construct()?;
    println!("constructed the importable record-stream requests; local chunk proofs run in tests");
    Ok(())
}
