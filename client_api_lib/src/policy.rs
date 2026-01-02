use http::header::{ACCEPT, CONTENT_TYPE};
use http::{HeaderMap, HeaderValue};

#[derive(Default)]
pub struct Policy {
    pub headers: HeaderMap,
    pub query: Vec<(String, String)>,
}

impl Policy {
    pub fn new() -> Self {
        Self {
            headers: HeaderMap::new(),
            query: Vec::new(),
        }
    }

    pub fn ensure_accept(&mut self, ct: &'static str) {
        if ct.is_empty() {
            return;
        }
        if !self.headers.contains_key(ACCEPT) {
            self.headers.insert(ACCEPT, HeaderValue::from_static(ct));
        }
    }

    pub fn has_content_type(&self) -> bool {
        self.headers.contains_key(CONTENT_TYPE)
    }
}
