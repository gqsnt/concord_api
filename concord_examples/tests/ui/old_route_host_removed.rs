use concord_macros::api;

api! {
    client OldRouteHostSyntax {
        base https "example.com"

        rate_limit app {
            bucket application by [route.host] {
                10 / 1s
            }
        }
    }
}

fn main() {}
