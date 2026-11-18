// The bindings that the author's expressions see sit together in blocks: an owner, the
// fields of a status arm, the inputs of a part and of the node. A field of an arm may take
// the name of an owner, and two groups of entries may both name the client.
concord::api! {
    pub client Sample(key: String) {
        base "https://a.test";
        read concord::Json;
        part keyed(tag?: String) {
            query { "client_key" = client.key.len(); tag; }
        }
        scope zone(name: String) {
            GET f(..keyed) -> u16 {
                query { "zone" = zone.name.len(); "key" = client.key.len(); }
                status { 200 { status: zone } => zone.as_u16(), 201 { status: client } => client.as_u16() }
            }
            GET list(start?: u32, ..keyed) -> Vec<u32> {
                query { start; "zone" = zone.name.len(); }
                paginate concord::by_offset(start);
            }
        }
    }
}

fn main() {
    let request = Sample::new("k").zone("z").f().tag("t").request().unwrap();
    assert_eq!(request.url().query(), Some("client_key=1&tag=t&zone=1&key=1"));
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let call = Sample::new("k").zone("z").f();
    let value = runtime.block_on(call.read_response(concord::reply(200, ""))).unwrap();
    assert_eq!(value, 200);
    let call = Sample::new("k").zone("z").f();
    assert_eq!(runtime.block_on(call.read_response(concord::reply(201, ""))).unwrap(), 201);
    let _ = Sample::new("k").zone("z").list().pages();
}
