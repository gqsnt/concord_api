# Predictions

## Task 1 — required root value and construction
From the README example, I expect `client Trial(token: String)` to generate `TrialConfig { token }`, and `Trial::new(http, TrialConfig { token })` to construct it. I expect the generated config to bundle required root arguments for construction, rather than treating the token as a transport setting.

## Task 2 — JSON and output shapes
From the README and Quick start, I expect `use concord::Json; format Json;` and a serde `Deserialize` output type to work. An endpoint without `->` should accept any successful status without decoding, whereas `-> ()` asks JSON to decode the body as unit and therefore still checks Content-Type and body syntax.

## Task 3 — borrowed and inferred arguments
Quick start says `impl IntoValue<String>` accepts `&str`, so I expect a borrowed `&str` to work for a declared `String`. I expect `"42".parse()` and `"name".into()` may lack an inferred destination because the method accepts `impl IntoValue<T>` rather than exactly `T`; explicit type annotations or passing literals directly should fix it.

## Task 4 — unread responses
Based on README and Quick start, I expect a 200 with `text/plain` for a JSON output to produce `ErrorKind::Metadata`, and a 4xx to produce `ErrorKind::Status`. In both cases `into_response()` should return the unread native response so `.bytes().await` can recover the exact body. I will use a local finite server and timeout.

## Task 5 — move shared default into a scope
I expect I can move two identical endpoint `headers`/`query` defaults into their parent scope; a third endpoint can override the same key locally. I expect replacements to keep their inherited key's position, so prepared requests should preserve both values and query order, though moving a default might affect its position relative to other keys. I will compare before and after explicitly.
