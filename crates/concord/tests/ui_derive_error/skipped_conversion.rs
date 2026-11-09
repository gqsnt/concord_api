// Salesforce's `Api` payload is read asynchronously by `from_concord`: no `From` is generated.
pub struct ApiFailure;

#[derive(Debug, concord::Error)]
pub enum SfError {
    #[display("{0}")]
    Concord(concord::Error),
    #[display("provider failure")]
    #[from(skip)]
    Api(Box<ApiFailure>),
}

impl std::fmt::Debug for ApiFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiFailure")
    }
}

fn main() {
    let _: SfError = ApiFailure.into();
}
