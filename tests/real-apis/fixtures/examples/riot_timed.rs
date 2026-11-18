async fn timed<E: riot::Endpoint>(call: riot::Call<E>) -> riot::Result<E::Value> {
    let started = std::time::Instant::now();
    let result = call.send().await;
    let _elapsed = started.elapsed();
    result
}
