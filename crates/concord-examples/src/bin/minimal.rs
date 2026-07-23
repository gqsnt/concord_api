#![forbid(unsafe_code)]

fn main() -> Result<(), concord::ClientBuildError> {
    let _request = concord_examples::getting_started::minimal::construct()?;
    println!("constructed the importable minimal health request");
    Ok(())
}
