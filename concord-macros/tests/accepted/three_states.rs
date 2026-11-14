use concord::json;
pub mod three {
    use super::*;
    concord::api! {
        pub client Three {
            base "https://t.example";
            POST go(milestone?: Option<u64>) { query { milestone; } body json { milestone; } }
        }
    }
    fn show(r: concord::reqwest::Request) -> (String, String) {
        (r.url().query().unwrap_or("").to_string(), String::from_utf8(r.body().unwrap().as_bytes().unwrap().to_vec()).unwrap())
    }
    pub fn check() {
        assert_eq!(show(Three::new().go().request().unwrap()), ("".into(), "{}".into()));
        assert_eq!(show(Three::new().go().milestone(None::<u64>).request().unwrap()), ("".into(), "{\"milestone\":null}".into()));
        assert_eq!(show(Three::new().go().milestone(5).request().unwrap()), ("milestone=5".into(), "{\"milestone\":5}".into()));
        assert_eq!(show(Three::new().go().milestone(Some(5)).request().unwrap()), ("milestone=5".into(), "{\"milestone\":5}".into()));
        // what really clears it:
        assert_eq!(show(Three::new().go().milestone(5).milestone(None::<Option<u64>>).request().unwrap()), ("".into(), "{}".into()));
    }
}

fn main() {three::check();}
