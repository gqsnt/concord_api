fn find_query_key_for_ep_field<'a>(ep: &'a ResolvedEndpoint, field: &Ident) -> Option<&'a KeyResolved> {
    // Take the last matching query op (closest to the endpoint) if multiple exist.
    ep.policy
        .endpoint
        .query
        .iter()
        .rev()
        .chain(
            ep.policy
                .scopes
                .iter()
                .rev()
                .flat_map(|scope| scope.query.iter().rev()),
        )
        .find_map(|op| match op {
            PolicyOp::Set {
                key,
                value: PolicySetValue::Value(PublicValueKind::EpField(f))
                    | PolicySetValue::OptionalEpField(f),
                ..
            } if f == field => Some(key),
            _ => None,
        })
}




