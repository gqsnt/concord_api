part keyed(idempotency_key: String) { headers { "idempotency-key" = idempotency_key; } idempotent; }
