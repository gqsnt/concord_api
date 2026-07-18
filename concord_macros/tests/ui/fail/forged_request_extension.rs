use concord_macros::api;

api! {
    client SetterApi {
        base "http://setter.example.test";
        GET Item(verbose: Option<bool>) -> Bytes;
    }
}

struct Forged;

impl setter_api::endpoints::ItemRequestExt for Forged {
    fn __concord_request_extension_seal(
        &self,
        _: setter_api::endpoints::__concord_item_request_ext_seal::Token,
    ) {
    }

    fn verbose(self, _value: bool) -> Self {
        self
    }

    fn clear_verbose(self) -> Self {
        self
    }
}

fn main() {}
