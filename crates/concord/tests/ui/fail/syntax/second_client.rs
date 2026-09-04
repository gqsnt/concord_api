use concord::api;

api! {
    client FirstApi {
        timeout 30s;
        base "https://one.example.test";


    client SecondApi {
        base "https://two.example.test";
    }
}}

fn main() {}
