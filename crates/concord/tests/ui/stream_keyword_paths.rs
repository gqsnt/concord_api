// `stream` is the keyword only when it is not the start of a path, a generic type or a type
// macro: these are ordinary finite outputs, and a missing format is reported as such.
mod paths {
    pub mod stream {
        pub struct Event;
    }
    concord::api! {
        pub client Paths {
            base "http://x.test";
            GET path() -> stream::Event;
        }
    }
}
mod generic {
    #[allow(non_camel_case_types)]
    pub struct stream<T>(T);
    concord::api! {
        pub client Generic {
            base "http://x.test";
            GET generic() -> stream<u8>;
        }
    }
}
mod type_macro {
    #[allow(unused_macros)]
    macro_rules! stream {
        () => {
            String
        };
    }
    concord::api! {
        pub client Macro {
            base "http://x.test";
            GET mac() -> stream!();
        }
    }
}
fn main() {}
