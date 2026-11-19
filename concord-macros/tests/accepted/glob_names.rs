#![allow(non_camel_case_types)]

mod model {
    pub type __Problem = String;
    pub struct __View0;
    pub type __Impl1 = u32;
    pub type __DataImpl1 = u64;
    pub type __Types = u8;
    pub type __Pager = u8;
    pub type __PagesFor = u8;
    pub type __PagingKey = u8;
    pub type __Data4 = u8;
}

use api::*;
use model::*;

concord::api! {
    client Api {
        base "https://example.invalid";
        #[deprecated] GET retired();
        GET get() -> __Problem {
            query { "view" = std::any::type_name_of_val(&__View0); }
            read concord::Text;
        }
        scope nested {
            GET wide(a?: u8, b?: u8, c?: u8, d?: u8, e?: u8, f?: u8, g?: u8,
                     h?: u8, i?: u8, j?: u8, k?: u8, l?: u8, m?: u8) -> Vec<u8> {
                path "wide";
                query { a; b; c; d; e; f; g; h; i; j; k; l; m; }
                read concord::Json;
                paginate concord::by_offset(m);
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let _: __Impl1 = 1;
    let _: __DataImpl1 = 1;
    let _: (__Types, __Pager, __PagesFor, __PagingKey) = (1, 1, 1, 1);
    let api = Api::new().with_transport(concord::fake(|request| {
        if request.url().path() == "/wide" {
            assert_eq!(request.url().query(), Some("m=7"));
            Ok(concord::reply(200, "[7]"))
        } else {
            assert_eq!(
                request.url().query_pairs().next().unwrap().1,
                std::any::type_name::<model::__View0>()
            );
            Ok(concord::reply(200, "ok"))
        }
    }));
    let call: Call<Get> = api.get();
    assert_eq!(call.await.unwrap(), "ok");

    use api::nested::*;
    let _: __Data4 = 1;
    let call: Call<Wide> = api.nested().wide().m(7u8);
    assert_eq!(call.send().await.unwrap(), vec![7]);
}
