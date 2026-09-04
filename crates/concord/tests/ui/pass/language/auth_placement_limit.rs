use concord::{Empty, api};

api! {
    client AuthPlacementBoundary {
        timeout 30s;
        base "https://example.test";
        credentials {
            secret key_value;
            credential key = api_key(secret.key_value);
        }
        auth {
            query "auth-00" = key;
            query "auth-01" = key;
            query "auth-02" = key;
            query "auth-03" = key;
            query "auth-04" = key;
            query "auth-05" = key;
            query "auth-06" = key;
            query "auth-07" = key;
            query "auth-08" = key;
            query "auth-09" = key;
            query "auth-10" = key;
            query "auth-11" = key;
            query "auth-12" = key;
            query "auth-13" = key;
            query "auth-14" = key;
            query "auth-15" = key;
            query "auth-16" = key;
            query "auth-17" = key;
            query "auth-18" = key;
            query "auth-19" = key;
            query "auth-20" = key;
            query "auth-21" = key;
            query "auth-22" = key;
            query "auth-23" = key;
            query "auth-24" = key;
            query "auth-25" = key;
            query "auth-26" = key;
            query "auth-27" = key;
            query "auth-28" = key;
            query "auth-29" = key;
            query "auth-30" = key;
            query "auth-31" = key;
            query "auth-32" = key;
            query "auth-33" = key;
            query "auth-34" = key;
            query "auth-35" = key;
            query "auth-36" = key;
            query "auth-37" = key;
            query "auth-38" = key;
            query "auth-39" = key;
            query "auth-40" = key;
            query "auth-41" = key;
            query "auth-42" = key;
            query "auth-43" = key;
            query "auth-44" = key;
            query "auth-45" = key;
            query "auth-46" = key;
            query "auth-47" = key;
            query "auth-48" = key;
            query "auth-49" = key;
            query "auth-50" = key;
            query "auth-51" = key;
            query "auth-52" = key;
            query "auth-53" = key;
            query "auth-54" = key;
            query "auth-55" = key;
            query "auth-56" = key;
            query "auth-57" = key;
            query "auth-58" = key;
            query "auth-59" = key;
            query "auth-60" = key;
            query "auth-61" = key;
            query "auth-62" = key;
            query "auth-63" = key;
        }


    GET read() -> () via Empty;
}}

fn main() {}
