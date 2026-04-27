fn find_query_key_for_ep_field<'a>(ep: &'a EndpointIr, field: &Ident) -> Option<&'a KeyResolved> {
    // Take the last matching query op (closest to the endpoint) if multiple exist.
    ep.policy.query.iter().rev().find_map(|op| match op {
        PolicyOp::Set {
            key,
            value: ValueKind::EpField(f),
            ..
        } if f == field => Some(key),
        _ => None,
    })
}


