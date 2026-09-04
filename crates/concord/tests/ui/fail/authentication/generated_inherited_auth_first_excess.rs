use concord::api;

api! {
    client InheritedAuthPlacementFirstExcess {
        timeout 30s;
        base "https://example.test";
        credentials {
            secret key_value;
            credential key = api_key(secret.key_value);
        }
        auth { query "auth-00" = key; }
        auth { query "auth-01" = key; }
        auth { query "auth-02" = key; }
        auth { query "auth-03" = key; }
        auth { query "auth-04" = key; }
        auth { query "auth-05" = key; }
        auth { query "auth-06" = key; }
        auth { query "auth-07" = key; }
        auth { query "auth-08" = key; }
        auth { query "auth-09" = key; }
        auth { query "auth-10" = key; }
        auth { query "auth-11" = key; }
        auth { query "auth-12" = key; }
        auth { query "auth-13" = key; }
        auth { query "auth-14" = key; }
        auth { query "auth-15" = key; }
        auth { query "auth-16" = key; }
        auth { query "auth-17" = key; }
        auth { query "auth-18" = key; }
        auth { query "auth-19" = key; }
        auth { query "auth-20" = key; }
        auth { query "auth-21" = key; }
        auth { query "auth-22" = key; }
        auth { query "auth-23" = key; }
        auth { query "auth-24" = key; }
        auth { query "auth-25" = key; }
        auth { query "auth-26" = key; }
        auth { query "auth-27" = key; }
        auth { query "auth-28" = key; }
        auth { query "auth-29" = key; }
        auth { query "auth-30" = key; }
        auth { query "auth-31" = key; }


    scope secured() {
        auth { query "auth-32" = key; }
        auth { query "auth-33" = key; }
        auth { query "auth-34" = key; }
        auth { query "auth-35" = key; }
        auth { query "auth-36" = key; }
        auth { query "auth-37" = key; }
        auth { query "auth-38" = key; }
        auth { query "auth-39" = key; }
        auth { query "auth-40" = key; }
        auth { query "auth-41" = key; }
        auth { query "auth-42" = key; }
        auth { query "auth-43" = key; }
        auth { query "auth-44" = key; }
        auth { query "auth-45" = key; }
        auth { query "auth-46" = key; }
        auth { query "auth-47" = key; }
        auth { query "auth-48" = key; }
        auth { query "auth-49" = key; }
        auth { query "auth-50" = key; }
        auth { query "auth-51" = key; }
        auth { query "auth-52" = key; }
        auth { query "auth-53" = key; }
        auth { query "auth-54" = key; }
        auth { query "auth-55" = key; }
        auth { query "auth-56" = key; }
        auth { query "auth-57" = key; }
        auth { query "auth-58" = key; }
        auth { query "auth-59" = key; }
        auth { query "auth-60" = key; }
        auth { query "auth-61" = key; }
        auth { query "auth-62" = key; }
        auth { query "auth-63" = key; }

        GET read() -> () via concord::Empty {
            auth { query "auth-64" = key; }
        }
    }
}}

fn main() {}
