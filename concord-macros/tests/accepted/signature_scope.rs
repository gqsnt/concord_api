type E = u32;
type Inventory = u8;

#[allow(non_upper_case_globals)]
const list: usize = 3;

const fn bytes() -> usize {
    list
}

const fn inventory() -> usize {
    2
}

fn main() {
    #[allow(non_upper_case_globals)]
    const call: usize = inventory();

    concord::api! {
        client Catalog(key: E, priority?: Inventory) {
            base "https://catalog.test/";
            headers { "x-key" = key; "x-priority" = priority; }
            scope inventory(id: Inventory, alternate?: E) {
                query { id; alternate; }
                GET list(values: [u8; bytes()], bytes?: [u8; call]) {
                    query {
                        "values" = values.len();
                        "bytes" = bytes.as_ref().map(|input| input.len());
                        "author" = call + list;
                    }
                }
            }
        }
    }

    let inventory = Catalog::new(7_u32)
        .priority(1_u8)
        .inventory(9_u8)
        .alternate(8_u32);
    let call_value = inventory.list([1_u8; bytes()]).bytes([2_u8; call]);
    let request = catalog::inventory::List::bytes(call_value, [3_u8; call])
        .request()
        .unwrap();
    assert!(request
        .url()
        .query_pairs()
        .any(|(key, value)| key == "author" && value == "5"));
}
