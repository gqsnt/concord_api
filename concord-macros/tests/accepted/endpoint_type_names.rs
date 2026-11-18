use concord::Json;

type E = String;
type T = u32;
type Call = u16;
type Endpoint = u8;
type Value = String;
type Data = bool;
type Inspect = i32;
macro_rules! problem_type {
    () => {
        E
    };
}

concord::api! {
    pub client Named {
        base "https://names.test/";
        problem problem_type!() read Json;
        read Json;
        GET inspect(e: E, t: T, call: Call, endpoint: Endpoint, value: Value, data: Data, inspect: Inspect) -> Value {
            query { e; t; call; endpoint; value; data; inspect; }
        }
        GET optional() -> Option<problem_type!()> {
            status { 404 => None }
        }
        GET filter(call?: Call) {
            query { call; }
        }
    }
}

fn main() {
    block_scoped_client();
    let call: named::Call<named::Inspect> = Named::new().inspect("e", 2, 3, 4, "value", true, 5);
    let _ = call.renamed_request().unwrap();
    let _ = Named::new().optional().request().unwrap();
    let filtered = named::Filter::call(Named::new().filter(), 3_u16).call(4_u16);
    let _ = filtered.request().unwrap();
}

impl named::Call<named::Inspect> {
    fn renamed_request(self) -> named::Result<concord::reqwest::Request> {
        self.into_request()
    }
}

fn block_scoped_client() {
    #[allow(non_upper_case_globals)]
    const call: &str = "from-author";
    #[allow(non_upper_case_globals)]
    const retry: usize = 1;
    #[allow(non_upper_case_globals)]
    const etag: usize = 2;
    type ScopedProblem = [u8; retry + etag];
    concord::api! {
        client BlockScoped {
            base "https://names.test/";
            problem ScopedProblem read Json;
            GET get(count?: u32) {
                query { count; "author" = call; }
            }
        }
    }
    let request = block_scoped::Get::count(BlockScoped::new().get(), 1_u32)
        .request()
        .unwrap();
    assert!(request
        .url()
        .query_pairs()
        .any(|(k, v)| k == "author" && v == call));
    let _ = BlockScoped::new()
        .get()
        .with_retry(concord::Retry::<ScopedProblem>::none())
        .send_if_none_match("tag");

    concord::api! {
        client DirectProblem {
            base "https://names.test/";
            problem [u8; retry + etag] read Json;
            GET get();
        }
    }
    let _ = DirectProblem::new()
        .get()
        .with_retry(concord::Retry::<ScopedProblem>::none())
        .send_if_none_match("tag");
}
