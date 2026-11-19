//! Blanket traits in scope leave the query and body that Concord sends alone.
mod dependency {
    pub trait Adapt {
        fn __adapt(self) -> &'static str
        where
            Self: Sized,
        {
            "foreign"
        }
    }
    impl<T> Adapt for T {}

    pub trait Push {
        fn __push<T>(&self, _: &'static str, _: T) {}
    }
    impl<T> Push for T {}
}

mod query {
    #[allow(unused_imports)]
    use super::dependency::Adapt as _;

    concord::api! {
        client Names {
            base "https://wire.test";
            GET get(n: u8) { query { n; } }
        }
    }

    pub async fn check() {
        let api = Names::new().with_transport(concord::fake(|request| {
            assert_eq!(request.url().query(), Some("n=7"));
            Ok(concord::reply(204, ""))
        }));
        api.get(7u8).await.unwrap();
    }
}

mod body {
    #[allow(unused_imports)]
    use super::dependency::Push as _;

    concord::api! {
        client Names {
            base "https://wire.test";
            POST put(n: u8) { body concord::json { n; } }
        }
    }

    pub async fn check() {
        let api = Names::new().with_transport(concord::fake(|request| {
            assert_eq!(request.body().unwrap().as_bytes().unwrap(), b"{\"n\":7}");
            Ok(concord::reply(204, ""))
        }));
        api.put(7u8).await.unwrap();
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("query") => query::check().await,
        Some("body") => body::check().await,
        _ => {
            query::check().await;
            body::check().await;
        }
    }
}
