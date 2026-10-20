use concord::api;

api! { client DefaultAttr { base "https://example.test/"; values { #[defualt] page: u32; } } }
api! { client SensitiveAttr { base "https://example.test/"; values { #[Sensitive] token: String; } } }
api! { #[deprectaed] client DeprecatedAttr { base "https://example.test/"; } }
api! { #[dco = "example"] client DocAttr { base "https://example.test/"; } }
api! { #[deprecated(snice = "1.0")] client DeprecatedField { base "https://example.test/"; } }
// `sensitive` is not valid on declarations and must not be suggested here.
api! { #[sensitve] client InvalidAttrCandidate { base "https://example.test/"; } }

fn main() {}
