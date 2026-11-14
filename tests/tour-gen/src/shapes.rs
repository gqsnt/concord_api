concord::api! {
/// A client whose endpoints have the shapes that the tests of one call need.
///
/// # Example
///
/// ```no_run
/// use tour::Shapes;
///
/// # async fn example() -> tour::shapes::Result<()> {
/// let shapes = Shapes::new();
/// let item = shapes.wrapped().get(1).await?;
/// # Ok(())
/// # }
/// ```
///
pub client Shapes {
    read Json;
    base "https://shapes.example.com";
    problem ShapesProblem;
    /// Send an item again. This `POST` may be repeated.
    POST again(
        /// The identifier of the item.
        id: u64,
    ) -> Item {
        path "again/{id}";
        idempotent;
    }
    /// Replace an item. This `PUT` may not be repeated.
    PUT replace(
        /// The identifier of the item to replace.
        id: u64,
        /// The new item.
        item: Item,
    ) -> Item {
        path "items/{id}";
        body json(item);
        idempotent false;
    }
    /// Note when each expression of a send runs.
    POST order(
        /// What each expression notes, and what it gives.
        trace: Trace,
    ) -> Item {
        path "order";
        query { "step" = trace.note("query"); }
        idempotent trace.idempotent();
        idle_timeout trace.idle_timeout();
    }
    /// Upload a body that the caller gives: bytes, or a stream.
    POST upload(
        /// The body to send.
        body: concord::Body,
    ) -> Item {
        path "upload";
        body body;
    }
    /// An item, or the conflict that holds it.
    GET outcome(
        /// The identifier of the item.
        id: u64,
    ) -> Result<Item, Conflict> {
        path "outcome/{id}";
        status { 409 { body } => Err(body) }
    }
    /// Check that the service answers.
    HEAD ping() {
        path "ping";
    }
    /// Whether an item is present. Only a `204` says yes.
    HEAD present(
        /// The identifier of the item.
        id: u64,
    ) -> bool {
        path "items/{id}";
        status { 204 => true, 404 => false }
    }
    /// Create an item, and keep where it was put.
    POST create(
        /// The item to create.
        item: Item,
    ) -> Receipt {
        path "items";
        body json(item);
        status {
            201 { headers, status } => Receipt {
                status: status.as_u16(),
                location: Some(headers.require("location")?.to_owned()),
            },
            202 { status: code } => Receipt { status: code.as_u16(), location: None },
            3xx => Err(concord::mismatch("a redirect is not a receipt"))?,
        }
    }
    /// An item, on the host of a tenant.
    GET hosted(
        /// The tenant: one label of the host.
        tenant: String,
    ) -> Item {
        base "https://{tenant}.shapes.example.com";
        path "tenant";
    }
    /// Search with a number, and a filter that a query cannot hold.
    GET search(
        /// A number, written in the query as JSON writes it.
        ratio: f64,
        /// A struct: a query refuses it when the request is built.
        filter?: Filter,
    ) -> Vec<Item> {
        path "search";
        query { ratio; filter; }
    }
    /// Send an item with a length written by hand, in a body that may fail.
    POST measured(
        /// The item. One with an empty title is not written.
        item: Item,
        /// The content-length to send.
        length: u64,
    ) -> Item {
        path "measured";
        headers { "content-length" = length; }
        body checked(item);
    }
    /// An item, asked with a number that is parsed when the request is built.
    GET parsed(
        /// The number, as text.
        raw: String,
    ) -> Item {
        path "parsed";
        query { "n" = raw.parse::<u32>()?; }
    }
    /// Counts by name.
    GET counts() -> Tally {
        path "counts";
    }
    /// An item, read only when the answer is labelled JSON.
    GET strict() -> Item {
        path "strict";
        read Json.strict();
    }
    /// An item, read by a reader whose message is long.
    GET long_message() -> Item {
        path "long-message";
        read Chatty;
    }
    /// Items in an envelope.
    scope wrapped {
        path "wrapped";
        status { 2xx { body } => unpack(body)? }
        /// An item, out of its envelope.
        GET get(
            /// The identifier of the item.
            id: u64,
        ) -> Item {
            path "{id}";
        }
        /// The envelope itself.
        GET raw(
            /// The identifier of the item.
            id: u64,
        ) -> Envelope<Item> {
            path "raw/{id}";
            status { remove 2xx }
        }
        /// An item out of its envelope, or none.
        GET find(
            /// The identifier of the item.
            id: u64,
        ) -> Option<Item> {
            path "find/{id}";
            status { override 2xx { body } => Some(unpack(body)?), 404 => None }
        }
        /// Touch an item. An accepted touch is not done; a missing item is fine.
        HEAD touch(
            /// The identifier of the item.
            id: u64,
        ) {
            path "touch/{id}";
            status {
                override 202 => Err(concord::mismatch("an accepted touch is not done"))?,
                404 => (),
            }
        }
        /// Whether an item is held. Only a `204` says yes.
        HEAD holds(
            /// The identifier of the item.
            id: u64,
        ) -> bool {
            path "holds/{id}";
            status { override 204 => true, 404 => false }
        }
    }
    /// An item, read by a reader that notes each answer it reads.
    GET noted(
        /// What the reader notes.
        trace: Trace,
    ) -> Item {
        path "noted";
        read { trace.note("read"); Json };
    }
    /// An item; a missing one is made of what was asked.
    GET titled(
        /// The identifier of the item.
        id: u64,
        /// The title of a missing item.
        title: String,
    ) -> Item {
        path "titled/{id}";
        status { 404 => Item { id: *id, title: title.clone() } }
    }
    /// An item, or the conflict that holds it, asked by an input named `body`.
    GET held(
        /// The identifier of the item. Its name is the field of an arm.
        body: u64,
    ) -> Result<Item, Conflict> {
        path "held/{body}";
        status {
            409 { body: answer } => {
                let conflict: Conflict = answer;
                Err(Conflict { message: format!("item {body}: {}", conflict.message) })
            }
        }
    }
    /// An item. A refusal is read by the arm of the endpoint, and ends with its problem.
    GET refusal(
        /// The identifier of the item.
        id: u64,
    ) -> Item {
        path "refusal/{id}";
        status { 4xx { body } => Err::<_, ShapesProblem>(body)? }
    }
    /// Pages of items behind three keys: one in the path, one in the query, one in a header.
    GET guarded(
        /// A key written in the path.
        path_key: Secret,
        /// A key written as the query pair `key`.
        query_key?: Secret,
        /// A key written in the header `x-key`.
        header_key?: Secret,
        /// The cursor of the page.
        cursor?: String,
    ) -> Batch {
        path "guarded/{path_key}";
        query { "key" = query_key; cursor; }
        headers { "x-key" = header_key; }
        paginate next_batch(cursor);
    }
    /// Pages of items whose cursor goes in a header.
    GET headed(
        /// The cursor of the page, sent as `x-cursor`.
        cursor?: String,
    ) -> Batch {
        path "headed";
        headers { "x-cursor" = cursor; }
        paginate next_batch(cursor);
    }
    /// Pages of items asked with a body of entries: a token and the cursor.
    POST posted(
        /// A token written in the body.
        token?: Secret,
        /// The cursor of the page, written in the body.
        cursor?: String,
    ) -> Batch {
        path "posted";
        body json { token; cursor; }
        paginate next_batch(cursor);
    }
    /// Pages of items asked with a body that the caller gives: bytes, or a stream.
    POST streamed(
        /// The body of every page.
        file: concord::Body,
        /// The cursor of the page.
        cursor?: String,
    ) -> Batch {
        path "streamed";
        query { cursor; }
        body file;
        paginate next_batch(cursor);
    }
    /// Pages of items asked with a body that is a new stream for each send.
    POST renewed(
        /// The cursor of the page.
        cursor?: String,
    ) -> Batch {
        path "renewed";
        query { cursor; }
        body stream();
        paginate next_batch(cursor);
    }
    /// Pages of items on a host whose first label is a key.
    GET keyed_host(
        /// A key written as the first label of the host.
        host_key: Secret,
        /// The cursor of the page.
        cursor?: String,
    ) -> Batch {
        base "https://{host_key}.shapes.example.com";
        path "keyed";
        query { cursor; }
        paginate next_batch(cursor);
    }
    /// Pages of items whose pager moves the tenant, which stands in the host.
    GET roaming(
        /// The tenant: one label of the host.
        tenant: String,
        /// The cursor of the page.
        cursor?: String,
    ) -> Batch {
        base "https://{tenant}.shapes.example.com";
        path "roaming";
        query { cursor; }
        paginate move |call, page| page.value().next.clone().map(|next| tenant.set(call, next));
    }
    /// Pages of items that start at an offset.
    GET offset(
        /// The index of the first item.
        start?: u32,
    ) -> Vec<Item> {
        path "offset";
        query { start; }
        paginate by_offset(start);
    }
    /// Pages of items asked by their number.
    GET numbered(
        /// The number of the page; the first is 1.
        page?: u32,
    ) -> Vec<Item> {
        path "numbered";
        query { page; }
        paginate by_page(page, 1);
    }
    /// Pages of items whose page gives the address of the next one.
    GET addressed() -> Batch {
        path "addressed";
        paginate by_next_url(Batch::next_url);
    }
    /// Remove an item. An accepted removal is not done; a missing item is removed.
    DELETE remove(
        /// The identifier of the item.
        id: u64,
    ) {
        path "items/{id}";
        status {
            202 => Err(concord::mismatch("an accepted removal is not done"))?,
            404 => (),
        }
    }
    /// An item, or the conflict that holds it. An accepted change is a conflict too.
    GET settled(
        /// The identifier of the item.
        id: u64,
    ) -> Result<Item, Conflict> {
        path "settled/{id}";
        status { 202 => Err(Conflict { message: "pending".to_owned() }) }
    }
    /// A part of a file, as it comes, or none when there is nothing to send.
    GET part(
        /// The identifier of the part.
        id: u64,
    ) -> Option<concord::ByteStream> {
        path "parts/{id}";
        read Stream;
        status { 204 => None, 404 => None }
    }
    /// Files whose answers carry a seal.
    scope sealed {
        path "sealed";
        status { 2xx { body, headers } => unseal(body, headers)? }
        /// A file, as it comes, once the seal of its answer is checked.
        GET file(
            /// The identifier of the file.
            id: u64,
        ) -> concord::ByteStream {
            path "{id}";
            read Stream;
        }
    }
}
}
