use concord::api;

api! {
    client FirstApi {
        timeout off;
        base "https://one.example.test";


    client SecondApi {
        base "https://two.example.test";
    }
}}

fn main() {}
