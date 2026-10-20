use concord::api;

api! { client Windows { base "https://example.test/"; rate_limits { window limits { allow 1 per 1s; } } } }
api! { client Policy { base "https://example.test/"; rate_limits { polciy reads() { id "reads"; key [origin]; allow 1 per 1s; } } } }
api! { client Allow { base "https://example.test/"; rate_limits { windows limits { allwo 1 per 1s; } } } }
api! { client Per { base "https://example.test/"; rate_limits { windows limits { allow 1 pre 1s; } } } }
api! { client Id { base "https://example.test/"; rate_limits { policy reads() { ID "reads"; key [origin]; allow 1 per 1s; } } } }
api! { client Key { base "https://example.test/"; rate_limits { policy reads() { id "reads"; kye [origin]; allow 1 per 1s; } } } }
api! { client Origin { base "https://example.test/"; rate_limits { policy reads() { id "reads"; key [orgin]; allow 1 per 1s; } } } }
api! { client Endpoint { base "https://example.test/"; GET read() -> () { rate_limit { id "reads"; key [endpiont]; allow 1 per 1s; } } } }
api! { client FormalVia { base "https://example.test/"; rate_limits { policy reads(tenant) { id "reads"; key ["tenant" = tenant vai Encoder]; allow 1 per 1s; } } } }
api! { client Cost { base "https://example.test/"; rate_limits { policy reads() { id "reads"; key [origin]; cots 1; allow 1 per 1s; } } } }
api! { client Using { base "https://example.test/"; rate_limits { policy reads() { id "reads"; key [origin]; usnig limits; } } } }
api! { client NamedCost { base "https://example.test/"; rate_limit reads() { cots 1; } } }
api! { client ShortKey { base "https://example.test/"; rate_limit "reads" kye [origin] using limits; } }
api! { client ShortUsing { base "https://example.test/"; rate_limit "reads" key [origin] usnig limits; } }

// Endpoint dimensions are invalid on client/scope attachments and must not be suggested.
api! { client InvalidClientDimension { base "https://example.test/"; rate_limit { id "reads"; key [endpiont]; allow 1 per 1s; } } }
api! { client InvalidScopeDimension { base "https://example.test/"; scope region() { rate_limit "reads" key [endpiont] using limits; } } }

fn main() {}
