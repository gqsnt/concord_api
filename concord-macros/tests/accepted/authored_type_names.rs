type E = String;
type T = u32;
type Call = u16;
type Endpoint = i64;
type Value = Vec<i32>;
type Data = bool;
type Top = bool;

macro_rules! problem_type {
    () => {
        E
    };
}

concord::api! {
    client Hygiene {
        base "https://hygiene.example";
        read concord::Json;
        problem problem_type!();
        GET top(label: E, enabled: Data, code: Call, method: Endpoint, active: Top, call?: T) -> Value {
            query { label; enabled; code; method; active; call; }
        }
    }
}

fn main() {
    let initial = Hygiene::new().top("author", false, 7, -13, true);
    let call: hygiene::Call<hygiene::Top> = hygiene::Top::call(initial, 3);
    let _: concord::Sending<Value, E> = call.clone().send();
    assert_eq!(
        call.request().unwrap().url().query().unwrap(),
        "label=author&enabled=false&code=7&method=-13&active=true&call=3"
    );
}
