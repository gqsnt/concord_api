concord::api! { client A { base "https://a.example"; read concord::Json; GET arm(flag:bool) -> u32 {query {flag;} status {404 => {if *flag {return 17;} 18}}} } }
fn main() {}
