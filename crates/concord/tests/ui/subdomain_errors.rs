concord::api! {
    pub client Api {
        base "http://x.test";
        scope a { subdomain "under_score"; GET one(); }
        scope b { subdomain "-edge"; GET two(); }
        scope c { subdomain "x..y"; GET three(); }
    }
}
fn main() {}
