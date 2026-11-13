concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


part keyed(idempotency_key: String) { headers { "idempotency-key" = idempotency_key; } idempotent; }

POST fixture(..keyed);
} }