//! Descriptions resolve the author's unqualified types and unit structs.
#![allow(non_camel_case_types)]

use std::any::TypeId;

type __Input0 = String;
type __DeclaredProblem = u16;
type __Read1 = String;
type __Impl1 = u32;
type __DataImpl1 = u64;

struct __View0;

fn __body_function(entries: concord::body::Entries<'_>) -> impl concord::body::IntoBody + '_ {
    concord::json(entries)
}

fn __pager<C>(_: C) -> Option<C> {
    None
}

fn same_type<T: 'static, U: 'static>(_: U) -> bool {
    TypeId::of::<T>() == TypeId::of::<U>()
}

concord::api! {
    client Names(n: u8) {
        base "https://wire.test";
        problem String read concord::Text;
        query { n; }
        GET get() -> Option<__Read1> {
            query { "input" = same_type::<__Input0, String>(String::new());
                    "problem" = same_type::<__DeclaredProblem, u16>(0);
                    "view" = same_type::<crate::__View0, _>(__View0); }
            read concord::Text;
        }
    }
}

macro_rules! declared_problem {
    () => {
        __DeclaredProblem
    };
}

concord::api! {
    client MacroProblem {
        base "https://wire.test";
        problem declared_problem!() read concord::Json;
        GET get();
    }
}

concord::api! {
    client Deprecated {
        base "https://wire.test";
        #[deprecated] GET old();
        GET get() {
            query { "impl" = same_type::<__Impl1, u32>(0);
                    "data" = same_type::<__DataImpl1, u64>(0); }
        }
        POST put(n: u8) { path "put"; body __body_function { n; } }
        GET page() -> Vec<u8> {
            path "page";
            read concord::Json;
            paginate move |call, _| __pager(call);
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let api = Names::new(7u8).with_transport(concord::fake(|request| {
        assert_eq!(
            request.url().query(),
            Some("n=7&input=true&problem=true&view=true")
        );
        Ok(concord::reply(200, "body"))
    }));
    assert_eq!(api.get().await.unwrap().as_deref(), Some("body"));
    let api = MacroProblem::new()
        .with_retry(concord::Retry::<u16>::none())
        .with_transport(concord::fake(|_| Ok(concord::reply(404, "7"))));
    let error = api
        .get()
        .with_retry(concord::Retry::<u16>::none())
        .await
        .unwrap_err();
    assert_eq!(error.problem(), Some(&7));

    let api =
        Deprecated::new().with_transport(concord::fake(|request| match request.url().path() {
            "/put" => {
                assert_eq!(request.body().unwrap().as_bytes().unwrap(), b"{\"n\":7}");
                Ok(concord::reply(204, ""))
            }
            "/page" => Ok(concord::reply(200, "[7]")),
            "/" => {
                assert_eq!(request.url().query(), Some("impl=true&data=true"));
                Ok(concord::reply(204, ""))
            }
            path => panic!("unexpected request to {path}"),
        }));
    api.get().await.unwrap();
    api.put(7u8).await.unwrap();
    let mut pages = api.page().pages();
    assert_eq!(pages.try_next().await.unwrap(), Some(vec![7]));
    assert_eq!(pages.try_next().await.unwrap(), None);
}
