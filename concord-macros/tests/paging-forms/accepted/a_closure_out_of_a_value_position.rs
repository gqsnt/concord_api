//! A closure that does not give the expression its value leaves it certain.
#![allow(unused, unreachable_code, unused_braces, unused_parens, unused_labels, clippy::all)]
use concord::page::{Call, IntoItems, Pager, Paging};
use concord::{by_cursor, by_link, Answer, Fail, Json};

#[derive(serde::Deserialize)]
pub struct Page {
    pub items: Vec<u64>,
    pub next: Option<String>,
    pub more: bool,
}

impl IntoItems for Page {
    type Item = u64;
    fn into_items(self) -> Vec<u64> {
        self.items
    }
    fn count(&self) -> usize {
        self.items.len()
    }
}

const DESCENDING: bool = false;

/// A pager of the author's that ends every list.
pub struct Stop;
impl<C: Call> Pager<C> for Stop {
    fn next_call(&mut self, _: C, _: &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> {
        Ok(None)
    }
}

mod mine {
    use concord::page::Call;
    use concord::Answer;
    /// A function of the author's with the name of a shipped pager, which returns a closure.
    pub fn by_link<C: Call>() -> impl FnMut(C, &Answer<C::Value>) -> Option<C> + Send + 'static {
        |_, _| None
    }
}

macro_rules! link_pager {
    () => {
        ::concord::by_link()
    };
}
macro_rules! by_link {
    () => {
        ::concord::by_link()
    };
}
macro_rules! yes {
    () => {
        true
    };
}

concord::api! {
    pub client Forms {
        base "https://forms.test";
        read Json;
        GET list(after?: String, before?: String) -> Page {
            query { after; before; }
            paginate { let more = |page: &Page| page.more; by_cursor(after, |page| page.next.clone()).has_more(more) };
        }
    }
}

fn main() {
    let _ = Forms::new().list().pages();
}
