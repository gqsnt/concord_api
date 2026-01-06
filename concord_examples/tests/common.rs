use concord_core::internal::{PolicyPart, RoutePart};
use concord_core::prelude::*;
use core::time::Duration;

#[allow(unused)]
pub fn build_route_and_policy<Cx, E>(vars: Cx::Vars, ep: &E) -> (RouteParts, Policy)
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
{
    let client = ApiClient::<Cx>::new(vars.clone());

    let mut route = Cx::base_route(&vars);
    <<E as Endpoint<Cx>>::Route as RoutePart<Cx, E>>::apply(ep, &client, &mut route).unwrap();

    let mut policy = Cx::base_policy(&vars).unwrap_or_default();
    policy.set_layer(PolicyLayer::Endpoint);
    <<E as Endpoint<Cx>>::Policy as PolicyPart<Cx, E>>::apply(ep, &client, &mut policy).unwrap();

    (route, policy)
}

#[allow(unused)]
pub fn header(policy: &Policy, name: &'static str) -> Option<String> {
    let n = http::header::HeaderName::from_static(name);
    policy
        .headers()
        .get(n)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

#[allow(unused)]
pub fn timeout(policy: &Policy) -> Option<Duration> {
    policy.timeout()
}
