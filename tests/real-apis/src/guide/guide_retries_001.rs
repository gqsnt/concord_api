concord::api! {
pub client Payments {
        base "https://payments.example.com";
        POST create(amount: u64) {
            headers { "idempotency-key" = uuid::Uuid::new_v4().to_string(); }
            body concord::form { amount; }
            idempotent;
        }
    }
}
