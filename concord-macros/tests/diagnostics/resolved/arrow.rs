use concord::Json;

type MaybeNumber = Option<u32>;
type DomainNumber<T> = Result<T, String>;

mod model {
    pub struct Option<T>(pub T);
}

use model::Option as CustomOption;

concord::api! {
    client Aliased {
        base "https://types.test";
        read Json;
        GET optional() -> MaybeNumber { status { 404 => None } }
        GET domain() -> DomainNumber<u32> { status { 2xx => Ok(7) } }
    }
}

mod shadow {
    use super::CustomOption as Option;

    concord::api! {
        client Shadow {
            base "https://types.test";
            HEAD wrapped() -> Option<u32> { status { 2xx => Option(7) } }
        }
    }
}

fn main() {}
