#[macro_export]
macro_rules! application_api {
    ($($tokens:tt)*) => { const CONFIG: &str = stringify!($($tokens)*); };
}

std::cfg_select! {
    all() => {
        mod concord { pub(crate) use crate::application_api as api; }
    }
    _ => {}
}

use concord::api;
api! {
    client ForeignCfgSelect {
        base "https://foreign.example.test/";
        timeout 5s;
        GET read() -> () { path ["cfg-select-foreign"]; }
    }
}

fn main() {
    assert!(CONFIG.contains("[\"cfg-select-foreign\"]"), "{CONFIG}");
    println!("FOREIGN_CFG_SELECT {CONFIG}");
}
