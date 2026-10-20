use concord::api;

api! { client PathOff { base "https://example.test/"; path of; } }
api! { client PathReplace { base "https://example.test/"; path replcae "items"; } }
api! { client DnsOff { base "https://example.test/"; subdomain OFF; } }
api! { client QueryReplace { base "https://example.test/"; query replcae {} } }
api! { client KeyStatement { base "https://example.test/"; query { whenPresent "x" = value; } } }
api! { client Append { base "https://example.test/"; headers { apepnd "X-Trace" = value; } } }
api! { client Each { base "https://example.test/"; query { "x" = eac values; } } }
api! { client Concat { base "https://example.test/"; query { "x" = conact [value]; } } }
api! { client EmissionVia { base "https://example.test/"; query { "x" = value vai Encoder; } } }
api! { client NestedVia { base "https://example.test/"; query { "x" = concat [value vai Encoder]; } } }
api! { client HoleVia { base "https://example.test/"; path "items/{id vai Encoder}"; } }
api! { client Duration { base "https://example.test/"; timeout 1MS; } }

api! { client ReplaceStatement { base "https://example.test/"; query { replcae "x" = value; } } }
api! { client RemoveStatement { base "https://example.test/"; headers { remvoe "X-Trace"; } } }

fn main() {}
