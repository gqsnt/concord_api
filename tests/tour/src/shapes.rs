const _: () = {
    use ::concord::__private::AdaptValue as _;

    // The owner views have no inputs: neither the client nor the scope has one.
    struct __Client;
    struct __Wrapped;
    struct __Sealed;
    use concord::page::Slot;
    use concord::read::Stream;
    use concord::{by_next_url, by_offset, by_page, json, Json};

    // /// A client whose endpoints have the shapes that the tests of one call need.
    // ///
    // /// # Example
    // ///
    // /// ```no_run
    // /// use tour::Shapes;
    // ///
    // /// # async fn example() -> tour::shapes::Result<()> {
    // /// let shapes = Shapes::new();
    // /// let item = shapes.wrapped().get(1).await?;
    // /// # Ok(())
    // /// # }
    // /// ```
    // ///
    // pub client Shapes {
    //     read Json;
    //     base "https://shapes.example.com";
    //     problem ShapesProblem;
    //     /// Send an item again. This `POST` may be repeated.
    //     POST again(
    //         /// The identifier of the item.
    //         id: u64,
    //     ) -> Item {
    //         path "again/{id}";
    //         idempotent;
    //     }
    //     /// Replace an item. This `PUT` may not be repeated.
    //     PUT replace(
    //         /// The identifier of the item to replace.
    //         id: u64,
    //         /// The new item.
    //         item: Item,
    //     ) -> Item {
    //         path "items/{id}";
    //         body json(item);
    //         idempotent false;
    //     }
    //     /// Note when each expression of a send runs.
    //     POST order(
    //         /// What each expression notes, and what it gives.
    //         trace: Trace,
    //     ) -> Item {
    //         path "order";
    //         query { "step" = trace.note("query"); }
    //         idempotent trace.idempotent();
    //         idle_timeout trace.idle_timeout();
    //     }
    //     /// Upload a body that the caller gives: bytes, or a stream.
    //     POST upload(
    //         /// The body to send.
    //         body: concord::Body,
    //     ) -> Item {
    //         path "upload";
    //         body body;
    //     }
    //     /// An item, or the conflict that holds it.
    //     GET outcome(
    //         /// The identifier of the item.
    //         id: u64,
    //     ) -> Result<Item, Conflict> {
    //         path "outcome/{id}";
    //         status { 409 { body } => Err(body) }
    //     }
    //     /// Check that the service answers.
    //     HEAD ping() {
    //         path "ping";
    //     }
    //     /// Whether an item is present. Only a `204` says yes.
    //     HEAD present(
    //         /// The identifier of the item.
    //         id: u64,
    //     ) -> bool {
    //         path "items/{id}";
    //         status { 204 => true, 404 => false }
    //     }
    //     /// Create an item, and keep where it was put.
    //     POST create(
    //         /// The item to create.
    //         item: Item,
    //     ) -> Receipt {
    //         path "items";
    //         body json(item);
    //         status {
    //             201 { headers, status } => Receipt {
    //                 status: status.as_u16(),
    //                 location: Some(headers.require("location")?.to_owned()),
    //             },
    //             202 { status: code } => Receipt { status: code.as_u16(), location: None },
    //             3xx => Err(concord::mismatch("a redirect is not a receipt"))?,
    //         }
    //     }
    //     /// An item, on the host of a tenant.
    //     GET hosted(
    //         /// The tenant: one label of the host.
    //         tenant: String,
    //     ) -> Item {
    //         base "https://{tenant}.shapes.example.com";
    //         path "tenant";
    //     }
    //     /// Search with a number, and a filter that a query cannot hold.
    //     GET search(
    //         /// A number, written in the query as JSON writes it.
    //         ratio: f64,
    //         /// A struct: a query refuses it when the request is built.
    //         filter?: Filter,
    //     ) -> Vec<Item> {
    //         path "search";
    //         query { ratio; filter; }
    //     }
    //     /// Send an item with a length written by hand, in a body that may fail.
    //     POST measured(
    //         /// The item. One with an empty title is not written.
    //         item: Item,
    //         /// The content-length to send.
    //         length: u64,
    //     ) -> Item {
    //         path "measured";
    //         headers { "content-length" = length; }
    //         body checked(item);
    //     }
    //     /// An item, asked with a number that is parsed when the request is built.
    //     GET parsed(
    //         /// The number, as text.
    //         raw: String,
    //     ) -> Item {
    //         path "parsed";
    //         query { "n" = raw.parse::<u32>()?; }
    //     }
    //     /// Counts by name.
    //     GET counts() -> Tally {
    //         path "counts";
    //     }
    //     /// An item, read only when the answer is labelled JSON.
    //     GET strict() -> Item {
    //         path "strict";
    //         read Json.strict();
    //     }
    //     /// An item, read by a reader whose message is long.
    //     GET long_message() -> Item {
    //         path "long-message";
    //         read Chatty;
    //     }
    //     /// Items in an envelope.
    //     scope wrapped {
    //         path "wrapped";
    //         status { 2xx { body } => unpack(body)? }
    //         /// An item, out of its envelope.
    //         GET get(
    //             /// The identifier of the item.
    //             id: u64,
    //         ) -> Item {
    //             path "{id}";
    //         }
    //         /// The envelope itself.
    //         GET raw(
    //             /// The identifier of the item.
    //             id: u64,
    //         ) -> Envelope<Item> {
    //             path "raw/{id}";
    //             status { remove 2xx }
    //         }
    //         /// An item out of its envelope, or none.
    //         GET find(
    //             /// The identifier of the item.
    //             id: u64,
    //         ) -> Option<Item> {
    //             path "find/{id}";
    //             status { override 2xx { body } => Some(unpack(body)?), 404 => None }
    //         }
    //         /// Touch an item. An accepted touch is not done; a missing item is fine.
    //         HEAD touch(
    //             /// The identifier of the item.
    //             id: u64,
    //         ) {
    //             path "touch/{id}";
    //             status {
    //                 override 202 => Err(concord::mismatch("an accepted touch is not done"))?,
    //                 404 => (),
    //             }
    //         }
    //         /// Whether an item is held. Only a `204` says yes.
    //         HEAD holds(
    //             /// The identifier of the item.
    //             id: u64,
    //         ) -> bool {
    //             path "holds/{id}";
    //             status { override 204 => true, 404 => false }
    //         }
    //     }
    //     /// An item, read by a reader that notes each answer it reads.
    //     GET noted(
    //         /// What the reader notes.
    //         trace: Trace,
    //     ) -> Item {
    //         path "noted";
    //         read { trace.note("read"); Json };
    //     }
    //     /// An item; a missing one is made of what was asked.
    //     GET titled(
    //         /// The identifier of the item.
    //         id: u64,
    //         /// The title of a missing item.
    //         title: String,
    //     ) -> Item {
    //         path "titled/{id}";
    //         status { 404 => Item { id: *id, title: title.clone() } }
    //     }
    //     /// An item, or the conflict that holds it, asked by an input named `body`.
    //     GET held(
    //         /// The identifier of the item. Its name is the field of an arm.
    //         body: u64,
    //     ) -> Result<Item, Conflict> {
    //         path "held/{body}";
    //         status {
    //             409 { body: answer } => {
    //                 let conflict: Conflict = answer;
    //                 Err(Conflict { message: format!("item {body}: {}", conflict.message) })
    //             }
    //         }
    //     }
    //     /// An item. A refusal is read by the arm of the endpoint, and ends with its problem.
    //     GET refusal(
    //         /// The identifier of the item.
    //         id: u64,
    //     ) -> Item {
    //         path "refusal/{id}";
    //         status { 4xx { body } => Err::<_, ShapesProblem>(body)? }
    //     }
    //     /// Pages of items behind three keys: one in the path, one in the query, one in a header.
    //     GET guarded(
    //         /// A key written in the path.
    //         path_key: Secret,
    //         /// A key written as the query pair `key`.
    //         query_key?: Secret,
    //         /// A key written in the header `x-key`.
    //         header_key?: Secret,
    //         /// The cursor of the page.
    //         cursor?: String,
    //     ) -> Batch {
    //         path "guarded/{path_key}";
    //         query { "key" = query_key; cursor; }
    //         headers { "x-key" = header_key; }
    //         paginate next_batch(cursor);
    //     }
    //     /// Pages of items whose cursor goes in a header.
    //     GET headed(
    //         /// The cursor of the page, sent as `x-cursor`.
    //         cursor?: String,
    //     ) -> Batch {
    //         path "headed";
    //         headers { "x-cursor" = cursor; }
    //         paginate next_batch(cursor);
    //     }
    //     /// Pages of items asked with a body of entries: a token and the cursor.
    //     POST posted(
    //         /// A token written in the body.
    //         token?: Secret,
    //         /// The cursor of the page, written in the body.
    //         cursor?: String,
    //     ) -> Batch {
    //         path "posted";
    //         body json { token; cursor; }
    //         paginate next_batch(cursor);
    //     }
    //     /// Pages of items asked with a body that the caller gives: bytes, or a stream.
    //     POST streamed(
    //         /// The body of every page.
    //         file: concord::Body,
    //         /// The cursor of the page.
    //         cursor?: String,
    //     ) -> Batch {
    //         path "streamed";
    //         query { cursor; }
    //         body file;
    //         paginate next_batch(cursor);
    //     }
    //     /// Pages of items asked with a body that is a new stream for each send.
    //     POST renewed(
    //         /// The cursor of the page.
    //         cursor?: String,
    //     ) -> Batch {
    //         path "renewed";
    //         query { cursor; }
    //         body stream();
    //         paginate next_batch(cursor);
    //     }
    //     /// Pages of items on a host whose first label is a key.
    //     GET keyed_host(
    //         /// A key written as the first label of the host.
    //         host_key: Secret,
    //         /// The cursor of the page.
    //         cursor?: String,
    //     ) -> Batch {
    //         base "https://{host_key}.shapes.example.com";
    //         path "keyed";
    //         query { cursor; }
    //         paginate next_batch(cursor);
    //     }
    //     /// Pages of items whose pager moves the tenant, which stands in the host.
    //     GET roaming(
    //         /// The tenant: one label of the host.
    //         tenant: String,
    //         /// The cursor of the page.
    //         cursor?: String,
    //     ) -> Batch {
    //         base "https://{tenant}.shapes.example.com";
    //         path "roaming";
    //         query { cursor; }
    //         paginate move |call, page| page.value().next.clone().map(|next| tenant.set(call, next));
    //     }
    //     /// Pages of items that start at an offset.
    //     GET offset(
    //         /// The index of the first item.
    //         start?: u32,
    //     ) -> Vec<Item> {
    //         path "offset";
    //         query { start; }
    //         paginate by_offset(start);
    //     }
    //     /// Pages of items asked by their number.
    //     GET numbered(
    //         /// The number of the page; the first is 1.
    //         page?: u32,
    //     ) -> Vec<Item> {
    //         path "numbered";
    //         query { page; }
    //         paginate by_page(page, 1);
    //     }
    //     /// Pages of items whose page gives the address of the next one.
    //     GET addressed() -> Batch {
    //         path "addressed";
    //         paginate by_next_url(Batch::next_url);
    //     }
    //     /// Remove an item. An accepted removal is not done; a missing item is removed.
    //     DELETE remove(
    //         /// The identifier of the item.
    //         id: u64,
    //     ) {
    //         path "items/{id}";
    //         status {
    //             202 => Err(concord::mismatch("an accepted removal is not done"))?,
    //             404 => (),
    //         }
    //     }
    //     /// An item, or the conflict that holds it. An accepted change is a conflict too.
    //     GET settled(
    //         /// The identifier of the item.
    //         id: u64,
    //     ) -> Result<Item, Conflict> {
    //         path "settled/{id}";
    //         status { 202 => Err(Conflict { message: "pending".to_owned() }) }
    //     }
    //     /// A part of a file, as it comes, or none when there is nothing to send.
    //     GET part(
    //         /// The identifier of the part.
    //         id: u64,
    //     ) -> Option<concord::ByteStream> {
    //         path "parts/{id}";
    //         read Stream;
    //         status { 204 => None, 404 => None }
    //     }
    //     /// Files whose answers carry a seal.
    //     scope sealed {
    //         path "sealed";
    //         status { 2xx { body, headers } => unseal(body, headers)? }
    //         /// A file, as it comes, once the seal of its answer is checked.
    //         GET file(
    //             /// The identifier of the file.
    //             id: u64,
    //         ) -> concord::ByteStream {
    //             path "{id}";
    //             read Stream;
    //         }
    //     }
    // }
    impl shapes::__Types for () {
        type __Input0 = u64;
        type __Input1 = u64;
        type __Input2 = Item;
        type __Input3 = Trace;
        type __Input4 = ::concord::Body;
        type __Input5 = u64;
        type __Input6 = u64;
        type __Input7 = Item;
        type __Input8 = String;
        type __Input9 = f64;
        type __Input10 = ::core::option::Option<Filter>;
        type __Input11 = Item;
        type __Input12 = u64;
        type __Input13 = String;
        type __Input14 = u64;
        type __Input15 = u64;
        type __Input16 = u64;
        type __Input17 = u64;
        type __Input18 = u64;
        type __Input19 = Trace;
        type __Input20 = u64;
        type __Input21 = String;
        type __Input22 = u64;
        type __Input23 = u64;
        type __Input24 = Secret;
        type __Input25 = ::core::option::Option<Secret>;
        type __Input26 = ::core::option::Option<Secret>;
        type __Input27 = ::core::option::Option<String>;
        type __Input28 = ::core::option::Option<String>;
        type __Input29 = ::core::option::Option<Secret>;
        type __Input30 = ::core::option::Option<String>;
        type __Input31 = ::concord::Body;
        type __Input32 = ::core::option::Option<String>;
        type __Input33 = ::core::option::Option<String>;
        type __Input34 = Secret;
        type __Input35 = ::core::option::Option<String>;
        type __Input36 = String;
        type __Input37 = ::core::option::Option<String>;
        type __Input38 = ::core::option::Option<u32>;
        type __Input39 = ::core::option::Option<u32>;
        type __Input40 = u64;
        type __Input41 = u64;
        type __Input42 = u64;
        type __Input43 = u64;
        type __Problem = ShapesProblem;
    }

    impl Shapes {
        /// Create a client with the default transport and settings.
        // Preserve the hand-written generated client API without adding a Default implementation.
        #[allow(clippy::new_without_default)]
        pub fn new() -> Self {
            Self {
                __core: ::concord::__private::Core::default(),
            }
        }

        /// The retry policy for calls made by this client. Accepts a [`concord::Retry`]. Retries are off by default.
        pub fn with_retry(mut self, __retry: ::concord::Retry<ShapesProblem>) -> Self {
            self.__core = self.__core.with_retry(__retry);
            self
        }

        /// The longest a whole call may take, retries and reading included. No limit by default. Accepts a [`std::time::Duration`].
        pub fn with_timeout(mut self, __timeout: ::std::time::Duration) -> Self {
            self.__core = self.__core.with_timeout(__timeout);
            self
        }

        /// The longest the transport or a body read may be silent. A call override wins,
        /// then the description's `idle_timeout`, then this client setting, then 30 seconds.
        /// The description's expression still runs for every send, even with a call override.
        pub fn with_idle_timeout(mut self, __timeout: ::std::time::Duration) -> Self {
            self.__core = self.__core.with_idle_timeout(__timeout);
            self
        }

        /// Use this base URL for every request. A path prefix is allowed.
        ///
        /// Accepts [`concord::Base`], &[`concord::Base`], &[`str`], [`String`], or &[`String`]. Invalid text becomes an error when building the request.
        pub fn with_base(mut self, __base: impl ::concord::base::IntoBase) -> Self {
            self.__core = self.__core.with_base(__base);
            self
        }

        /// Choose what really sends, under this client's middlewares.
        ///
        /// Accepts a [`concord::Http`], a [`reqwest::Client`](https://docs.rs/reqwest/0.13.5/reqwest/struct.Client.html), a [`concord::fake`], or another [`concord::Transport`].
        pub fn with_transport(mut self, __transport: impl ::concord::Transport) -> Self {
            self.__core = self.__core.with_transport(__transport);
            self
        }

        /// Add a middleware before the description's, in the order added.
        ///
        /// It is made once and shared by this client's handles and calls.
        pub fn with_middleware(mut self, __middleware: impl ::concord::Middleware) -> Self {
            self.__core = self.__core.with_middleware(__middleware);
            self
        }

        /// Send an item again. This `POST` may be repeated.
        ///
        /// Call: `shapes.again(id).await?`.
        ///
        /// Request: `POST https://shapes.example.com/again/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        ///
        /// `.await` gives [`shapes::Result`]<[`Item`]>.
        pub fn again(&self, id: impl ::concord::arg::Arg<u64>) -> shapes::Again {
            shapes::Again {
                __client: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Replace an item. This `PUT` may not be repeated.
        ///
        /// Call: `shapes.replace(id, item).await?`.
        ///
        /// Request: `PUT https://shapes.example.com/items/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item to replace.
        /// - `item`: [`Item`] or &[`Item`]. The new item.
        ///
        /// Retry: not sent again, even when retry is on, unless the policy lists the
        /// response status with `Retry::not_applied_on`.
        ///
        /// `.await` gives [`shapes::Result`]<[`Item`]>.
        pub fn replace(
            &self,
            id: impl ::concord::arg::Arg<u64>,
            item: impl ::concord::arg::Arg<Item>,
        ) -> shapes::Replace {
            shapes::Replace {
                __client: self.clone(),
                id: id.into_input(),
                item: item.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Note when each expression of a send runs.
        ///
        /// Call: `shapes.order(trace).await?`.
        ///
        /// Request: `POST https://shapes.example.com/order`.
        ///
        /// Inputs:
        ///
        /// - `trace`: [`Trace`] or &[`Trace`]. What each expression notes, and what it gives.
        ///
        /// Retry: whether this call may be sent again is decided from its inputs, for each send.
        ///
        /// `.await` gives [`shapes::Result`]<[`Item`]>.
        pub fn order(&self, trace: impl ::concord::arg::Arg<Trace>) -> shapes::Order {
            shapes::Order {
                __client: self.clone(),
                trace: trace.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Upload a body that the caller gives: bytes, or a stream.
        ///
        /// Call: `shapes.upload(body).await?`.
        ///
        /// Request: `POST https://shapes.example.com/upload`.
        ///
        /// Inputs:
        ///
        /// - `body`: [`concord::Body`] or &[`concord::Body`]. The body to send.
        ///   Make one with [`concord::Body::new(bytes, "application/octet-stream")`](concord::Body::new)
        ///   or [`concord::Body::stream(stream)`](concord::Body::stream); a stream is sent once.
        ///
        /// Retry: an unmarked `POST` may be sent again when the policy lists the
        /// response status with `Retry::not_applied_on`; all other retry checks still apply.
        ///
        /// `.await` gives [`shapes::Result`]<[`Item`]>.
        pub fn upload(&self, body: impl ::concord::arg::Arg<::concord::Body>) -> shapes::Upload {
            shapes::Upload {
                __client: self.clone(),
                body: body.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// An item, or the conflict that holds it.
        ///
        /// Call: `shapes.outcome(id).await?`.
        ///
        /// Request: `GET https://shapes.example.com/outcome/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        ///
        /// Status: `409`.
        ///
        /// `.await` gives [`shapes::Result`]<[`Result`]<[`Item`], [`Conflict`]>>.
        pub fn outcome(&self, id: impl ::concord::arg::Arg<u64>) -> shapes::Outcome {
            shapes::Outcome {
                __client: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Check that the service answers.
        ///
        /// Call: `shapes.ping().await?`.
        ///
        /// Request: `HEAD https://shapes.example.com/ping`.
        ///
        /// `.await` gives [`shapes::Result`]<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>.
        pub fn ping(&self) -> shapes::Ping {
            shapes::Ping {
                __client: self.clone(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Whether an item is present. Only a `204` says yes.
        ///
        /// Call: `shapes.present(id).await?`.
        ///
        /// Request: `HEAD https://shapes.example.com/items/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        ///
        /// Status: `204`.
        ///
        /// Status: `404`.
        ///
        /// `.await` gives [`shapes::Result`]<[`bool`]>.
        pub fn present(&self, id: impl ::concord::arg::Arg<u64>) -> shapes::Present {
            shapes::Present {
                __client: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Create an item, and keep where it was put.
        ///
        /// Call: `shapes.create(item).await?`.
        ///
        /// Request: `POST https://shapes.example.com/items`.
        ///
        /// Inputs:
        ///
        /// - `item`: [`Item`] or &[`Item`]. The item to create.
        ///
        /// Status: `201`.
        ///
        /// Status: `202`.
        ///
        /// Status: `3xx`.
        ///
        /// Retry: an unmarked `POST` may be sent again when the policy lists the
        /// response status with `Retry::not_applied_on`; all other retry checks still apply.
        ///
        /// `.await` gives [`shapes::Result`]<[`Receipt`]>.
        pub fn create(&self, item: impl ::concord::arg::Arg<Item>) -> shapes::Create {
            shapes::Create {
                __client: self.clone(),
                item: item.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// An item, on the host of a tenant.
        ///
        /// Call: `shapes.hosted(tenant).await?`.
        ///
        /// Request: `GET https://{tenant}.shapes.example.com/tenant`.
        ///
        /// Inputs:
        ///
        /// - `tenant`: [`String`], &[`String`], or &[`str`]. The tenant: one label of the host.
        ///
        /// `.await` gives [`shapes::Result`]<[`Item`]>.
        pub fn hosted(&self, tenant: impl ::concord::arg::Arg<String>) -> shapes::Hosted {
            shapes::Hosted {
                __client: self.clone(),
                tenant: tenant.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Search with a number, and a filter that a query cannot hold.
        ///
        /// Call: `shapes.search(ratio).await?`.
        ///
        /// Request: `GET https://shapes.example.com/search`.
        ///
        /// Inputs:
        ///
        /// - `ratio`: [`f64`] or &[`f64`]. A number, written in the query as JSON writes it.
        ///
        /// Options:
        ///
        /// - `.filter(filter)`: accepts [`Filter`], &[`Filter`], or an [`Option`]<[`Filter`]>. `None` clears it. A struct: a query refuses it when the request is built.
        ///
        /// `.await` gives [`shapes::Result`]<[`Vec`]<[`Item`]>>.
        pub fn search(&self, ratio: impl ::concord::arg::Arg<f64>) -> shapes::Search {
            shapes::Search {
                __client: self.clone(),
                ratio: ratio.into_input(),
                filter: ::core::option::Option::None,
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Send an item with a length written by hand, in a body that may fail.
        ///
        /// Call: `shapes.measured(item, length).await?`.
        ///
        /// Request: `POST https://shapes.example.com/measured`.
        ///
        /// Headers: `content-length`.
        ///
        /// Inputs:
        ///
        /// - `item`: [`Item`] or &[`Item`]. The item. One with an empty title is not written.
        /// - `length`: [`u64`] or &[`u64`]. The content-length to send.
        ///
        /// Retry: an unmarked `POST` may be sent again when the policy lists the
        /// response status with `Retry::not_applied_on`; all other retry checks still apply.
        ///
        /// `.await` gives [`shapes::Result`]<[`Item`]>.
        pub fn measured(
            &self,
            item: impl ::concord::arg::Arg<Item>,
            length: impl ::concord::arg::Arg<u64>,
        ) -> shapes::Measured {
            shapes::Measured {
                __client: self.clone(),
                item: item.into_input(),
                length: length.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// An item, asked with a number that is parsed when the request is built.
        ///
        /// Call: `shapes.parsed(raw).await?`.
        ///
        /// Request: `GET https://shapes.example.com/parsed`.
        ///
        /// Inputs:
        ///
        /// - `raw`: [`String`], &[`String`], or &[`str`]. The number, as text.
        ///
        /// `.await` gives [`shapes::Result`]<[`Item`]>.
        pub fn parsed(&self, raw: impl ::concord::arg::Arg<String>) -> shapes::Parsed {
            shapes::Parsed {
                __client: self.clone(),
                raw: raw.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Counts by name.
        ///
        /// Call: `shapes.counts().await?`.
        ///
        /// Request: `GET https://shapes.example.com/counts`.
        ///
        /// `.await` gives [`shapes::Result`]<[`Tally`]>.
        pub fn counts(&self) -> shapes::Counts {
            shapes::Counts {
                __client: self.clone(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// An item, read only when the answer is labelled JSON.
        ///
        /// Call: `shapes.strict().await?`.
        ///
        /// Request: `GET https://shapes.example.com/strict`.
        ///
        /// `.await` gives [`shapes::Result`]<[`Item`]>.
        pub fn strict(&self) -> shapes::Strict {
            shapes::Strict {
                __client: self.clone(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// An item, read by a reader whose message is long.
        ///
        /// Call: `shapes.long_message().await?`.
        ///
        /// Request: `GET https://shapes.example.com/long-message`.
        ///
        /// `.await` gives [`shapes::Result`]<[`Item`]>.
        pub fn long_message(&self) -> shapes::LongMessage {
            shapes::LongMessage {
                __client: self.clone(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Items in an envelope.
        ///
        /// Nothing is sent yet.
        pub fn wrapped(&self) -> shapes::Wrapped {
            shapes::Wrapped {
                __client: self.clone(),
            }
        }

        /// An item, read by a reader that notes each answer it reads.
        ///
        /// Call: `shapes.noted(trace).await?`.
        ///
        /// Request: `GET https://shapes.example.com/noted`.
        ///
        /// Inputs:
        ///
        /// - `trace`: [`Trace`] or &[`Trace`]. What the reader notes.
        ///
        /// `.await` gives [`shapes::Result`]<[`Item`]>.
        pub fn noted(&self, trace: impl ::concord::arg::Arg<Trace>) -> shapes::Noted {
            shapes::Noted {
                __client: self.clone(),
                trace: trace.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// An item; a missing one is made of what was asked.
        ///
        /// Call: `shapes.titled(id, title).await?`.
        ///
        /// Request: `GET https://shapes.example.com/titled/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        /// - `title`: [`String`], &[`String`], or &[`str`]. The title of a missing item.
        ///
        /// Status: `404`.
        ///
        /// `.await` gives [`shapes::Result`]<[`Item`]>.
        pub fn titled(
            &self,
            id: impl ::concord::arg::Arg<u64>,
            title: impl ::concord::arg::Arg<String>,
        ) -> shapes::Titled {
            shapes::Titled {
                __client: self.clone(),
                id: id.into_input(),
                title: title.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// An item, or the conflict that holds it, asked by an input named `body`.
        ///
        /// Call: `shapes.held(body).await?`.
        ///
        /// Request: `GET https://shapes.example.com/held/{body}`.
        ///
        /// Inputs:
        ///
        /// - `body`: [`u64`] or &[`u64`]. The identifier of the item. Its name is the field of an arm.
        ///
        /// Status: `409`.
        ///
        /// `.await` gives [`shapes::Result`]<[`Result`]<[`Item`], [`Conflict`]>>.
        pub fn held(&self, body: impl ::concord::arg::Arg<u64>) -> shapes::Held {
            shapes::Held {
                __client: self.clone(),
                body: body.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// An item. A refusal is read by the arm of the endpoint, and ends with its problem.
        ///
        /// Call: `shapes.refusal(id).await?`.
        ///
        /// Request: `GET https://shapes.example.com/refusal/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        ///
        /// Status: `4xx`.
        ///
        /// `.await` gives [`shapes::Result`]<[`Item`]>.
        pub fn refusal(&self, id: impl ::concord::arg::Arg<u64>) -> shapes::Refusal {
            shapes::Refusal {
                __client: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Pages of items behind three keys: one in the path, one in the query, one in a header.
        ///
        /// Call: `shapes.guarded(path_key).send().await?`.
        ///
        /// Request: `GET https://shapes.example.com/guarded/{path_key}`.
        ///
        /// Headers: `x-key`.
        ///
        /// Inputs:
        ///
        /// - `path_key`: [`Secret`], &[`Secret`], [`String`], or &[`str`]. A key written in the path.
        ///
        /// Options:
        ///
        /// - `.query_key("key")`: accepts [`Secret`], &[`Secret`], [`String`], &[`str`], or an [`Option`]<[`Secret`]>. `None` clears it. A key written as the query pair `key`.
        /// - `.header_key("key")`: accepts [`Secret`], &[`Secret`], [`String`], &[`str`], or an [`Option`]<[`Secret`]>. `None` clears it. A key written in the header `x-key`.
        /// - `.cursor("next")`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. The cursor of the page.
        ///
        /// `.send().await` gives [`shapes::Result`]<[`Batch`]>.
        ///
        /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
        pub fn guarded(&self, path_key: impl ::concord::arg::Arg<Secret>) -> shapes::Guarded {
            shapes::Guarded {
                __client: self.clone(),
                path_key: path_key.into_input(),
                query_key: ::core::option::Option::None,
                header_key: ::core::option::Option::None,
                cursor: ::core::option::Option::None,
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Pages of items whose cursor goes in a header.
        ///
        /// Call: `shapes.headed().send().await?`.
        ///
        /// Request: `GET https://shapes.example.com/headed`.
        ///
        /// Headers: `x-cursor`.
        ///
        /// Options:
        ///
        /// - `.cursor("next")`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. The cursor of the page, sent as `x-cursor`.
        ///
        /// `.send().await` gives [`shapes::Result`]<[`Batch`]>.
        ///
        /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
        pub fn headed(&self) -> shapes::Headed {
            shapes::Headed {
                __client: self.clone(),
                cursor: ::core::option::Option::None,
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Pages of items asked with a body of entries: a token and the cursor.
        ///
        /// Call: `shapes.posted().send().await?`.
        ///
        /// Request: `POST https://shapes.example.com/posted`.
        ///
        /// Options:
        ///
        /// - `.token("key")`: accepts [`Secret`], &[`Secret`], [`String`], &[`str`], or an [`Option`]<[`Secret`]>. `None` clears it. A token written in the body.
        /// - `.cursor("next")`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. The cursor of the page, written in the body.
        ///
        /// `.send().await` gives [`shapes::Result`]<[`Batch`]>.
        ///
        /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
        pub fn posted(&self) -> shapes::Posted {
            shapes::Posted {
                __client: self.clone(),
                token: ::core::option::Option::None,
                cursor: ::core::option::Option::None,
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Pages of items asked with a body that the caller gives: bytes, or a stream.
        ///
        /// Call: `shapes.streamed(file).send().await?`.
        ///
        /// Request: `POST https://shapes.example.com/streamed`.
        ///
        /// Inputs:
        ///
        /// - `file`: [`concord::Body`] or &[`concord::Body`]. The body of every page.
        ///
        /// Options:
        ///
        /// - `.cursor("next")`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. The cursor of the page.
        ///
        /// `.send().await` gives [`shapes::Result`]<[`Batch`]>.
        ///
        /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
        pub fn streamed(&self, file: impl ::concord::arg::Arg<::concord::Body>) -> shapes::Streamed {
            shapes::Streamed {
                __client: self.clone(),
                file: file.into_input(),
                cursor: ::core::option::Option::None,
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Pages of items asked with a body that is a new stream for each send.
        ///
        /// Call: `shapes.renewed().send().await?`.
        ///
        /// Request: `POST https://shapes.example.com/renewed`.
        ///
        /// Options:
        ///
        /// - `.cursor("next")`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. The cursor of the page.
        ///
        /// `.send().await` gives [`shapes::Result`]<[`Batch`]>.
        ///
        /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
        pub fn renewed(&self) -> shapes::Renewed {
            shapes::Renewed {
                __client: self.clone(),
                cursor: ::core::option::Option::None,
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Pages of items on a host whose first label is a key.
        ///
        /// Call: `shapes.keyed_host(host_key).send().await?`.
        ///
        /// Request: `GET https://{host_key}.shapes.example.com/keyed`.
        ///
        /// Inputs:
        ///
        /// - `host_key`: [`Secret`], &[`Secret`], [`String`], or &[`str`]. A key written as the first label of the host.
        ///
        /// Options:
        ///
        /// - `.cursor("next")`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. The cursor of the page.
        ///
        /// `.send().await` gives [`shapes::Result`]<[`Batch`]>.
        ///
        /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
        pub fn keyed_host(&self, host_key: impl ::concord::arg::Arg<Secret>) -> shapes::KeyedHost {
            shapes::KeyedHost {
                __client: self.clone(),
                host_key: host_key.into_input(),
                cursor: ::core::option::Option::None,
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Pages of items whose pager moves the tenant, which stands in the host.
        ///
        /// Call: `shapes.roaming(tenant).send().await?`.
        ///
        /// Request: `GET https://{tenant}.shapes.example.com/roaming`.
        ///
        /// Inputs:
        ///
        /// - `tenant`: [`String`], &[`String`], or &[`str`]. The tenant: one label of the host.
        ///
        /// Options:
        ///
        /// - `.cursor("next")`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. The cursor of the page.
        ///
        /// `.send().await` gives [`shapes::Result`]<[`Batch`]>.
        ///
        /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
        pub fn roaming(&self, tenant: impl ::concord::arg::Arg<String>) -> shapes::Roaming {
            shapes::Roaming {
                __client: self.clone(),
                tenant: tenant.into_input(),
                cursor: ::core::option::Option::None,
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Pages of items that start at an offset.
        ///
        /// Call: `shapes.offset().send().await?`.
        ///
        /// Request: `GET https://shapes.example.com/offset`.
        ///
        /// Options:
        ///
        /// - `.start(5)`: accepts [`u32`], &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it. The index of the first item.
        ///
        /// `.send().await` gives [`shapes::Result`]<[`Vec`]<[`Item`]>>.
        ///
        /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
        pub fn offset(&self) -> shapes::Offset {
            shapes::Offset {
                __client: self.clone(),
                start: ::core::option::Option::None,
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Pages of items asked by their number.
        ///
        /// Call: `shapes.numbered().send().await?`.
        ///
        /// Request: `GET https://shapes.example.com/numbered`.
        ///
        /// Options:
        ///
        /// - `.page(5)`: accepts [`u32`], &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it. The number of the page; the first is 1.
        ///
        /// `.send().await` gives [`shapes::Result`]<[`Vec`]<[`Item`]>>.
        ///
        /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
        pub fn numbered(&self) -> shapes::Numbered {
            shapes::Numbered {
                __client: self.clone(),
                page: ::core::option::Option::None,
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Pages of items whose page gives the address of the next one.
        ///
        /// Call: `shapes.addressed().send().await?`.
        ///
        /// Request: `GET https://shapes.example.com/addressed`.
        ///
        /// `.send().await` gives [`shapes::Result`]<[`Batch`]>.
        ///
        /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
        pub fn addressed(&self) -> shapes::Addressed {
            shapes::Addressed {
                __client: self.clone(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Remove an item. An accepted removal is not done; a missing item is removed.
        ///
        /// Call: `shapes.remove(id).await?`.
        ///
        /// Request: `DELETE https://shapes.example.com/items/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        ///
        /// Status: `202`.
        ///
        /// Status: `404`.
        ///
        /// `.await` gives [`shapes::Result`]<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>.
        pub fn remove(&self, id: impl ::concord::arg::Arg<u64>) -> shapes::Remove {
            shapes::Remove {
                __client: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// An item, or the conflict that holds it. An accepted change is a conflict too.
        ///
        /// Call: `shapes.settled(id).await?`.
        ///
        /// Request: `GET https://shapes.example.com/settled/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        ///
        /// Status: `202`.
        ///
        /// `.await` gives [`shapes::Result`]<[`Result`]<[`Item`], [`Conflict`]>>.
        pub fn settled(&self, id: impl ::concord::arg::Arg<u64>) -> shapes::Settled {
            shapes::Settled {
                __client: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// A part of a file, as it comes, or none when there is nothing to send.
        ///
        /// Call: `shapes.part(id).await?`.
        ///
        /// Request: `GET https://shapes.example.com/parts/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the part.
        ///
        /// Status: `204`.
        ///
        /// Status: `404`.
        ///
        /// `.await` gives [`shapes::Result`]<[`Option`]<[`concord::ByteStream`]>>.
        pub fn part(&self, id: impl ::concord::arg::Arg<u64>) -> shapes::Part {
            shapes::Part {
                __client: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Files whose answers carry a seal.
        ///
        /// Nothing is sent yet.
        pub fn sealed(&self) -> shapes::Sealed {
            shapes::Sealed {
                __client: self.clone(),
            }
        }
    }

    impl ::std::fmt::Debug for Shapes {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Shapes").finish_non_exhaustive()
        }
    }

    // /// Items in an envelope.
    // scope wrapped {
    //     path "wrapped";
    //     status { 2xx { body } => unpack(body)? }
    //     /// An item, out of its envelope.
    //     GET get(
    //         /// The identifier of the item.
    //         id: u64,
    //     ) -> Item {
    //         path "{id}";
    //     }
    //     /// The envelope itself.
    //     GET raw(
    //         /// The identifier of the item.
    //         id: u64,
    //     ) -> Envelope<Item> {
    //         path "raw/{id}";
    //         status { remove 2xx }
    //     }
    //     /// An item out of its envelope, or none.
    //     GET find(
    //         /// The identifier of the item.
    //         id: u64,
    //     ) -> Option<Item> {
    //         path "find/{id}";
    //         status { override 2xx { body } => Some(unpack(body)?), 404 => None }
    //     }
    // }
    impl shapes::Wrapped {
        /// An item, out of its envelope.
        ///
        /// Call: `shapes.wrapped().get(id).await?`.
        ///
        /// Request: `GET https://shapes.example.com/wrapped/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        ///
        /// Status: `2xx`.
        ///
        /// `.await` gives [`shapes::Result`]<[`Item`]>.
        pub fn get(&self, id: impl ::concord::arg::Arg<u64>) -> shapes::wrapped::Get {
            shapes::wrapped::Get {
                __scope: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// The envelope itself.
        ///
        /// Call: `shapes.wrapped().raw(id).await?`.
        ///
        /// Request: `GET https://shapes.example.com/wrapped/raw/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        ///
        /// `.await` gives [`shapes::Result`]<[`Envelope`]<[`Item`]>>.
        pub fn raw(&self, id: impl ::concord::arg::Arg<u64>) -> shapes::wrapped::Raw {
            shapes::wrapped::Raw {
                __scope: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// An item out of its envelope, or none.
        ///
        /// Call: `shapes.wrapped().find(id).await?`.
        ///
        /// Request: `GET https://shapes.example.com/wrapped/find/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        ///
        /// Status: `2xx`.
        ///
        /// Status: `404`.
        ///
        /// `.await` gives [`shapes::Result`]<[`Option`]<[`Item`]>>.
        pub fn find(&self, id: impl ::concord::arg::Arg<u64>) -> shapes::wrapped::Find {
            shapes::wrapped::Find {
                __scope: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Touch an item. An accepted touch is not done; a missing item is fine.
        ///
        /// Call: `shapes.wrapped().touch(id).await?`.
        ///
        /// Request: `HEAD https://shapes.example.com/wrapped/touch/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        ///
        /// Status: `202`.
        ///
        /// Status: `404`.
        ///
        /// `.await` gives [`shapes::Result`]<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>.
        pub fn touch(&self, id: impl ::concord::arg::Arg<u64>) -> shapes::wrapped::Touch {
            shapes::wrapped::Touch {
                __scope: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Whether an item is held. Only a `204` says yes.
        ///
        /// Call: `shapes.wrapped().holds(id).await?`.
        ///
        /// Request: `HEAD https://shapes.example.com/wrapped/holds/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        ///
        /// Status: `204`.
        ///
        /// Status: `404`.
        ///
        /// `.await` gives [`shapes::Result`]<[`bool`]>.
        pub fn holds(&self, id: impl ::concord::arg::Arg<u64>) -> shapes::wrapped::Holds {
            shapes::wrapped::Holds {
                __scope: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }
    }

    impl ::std::fmt::Debug for shapes::Wrapped {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("shapes::Wrapped").finish_non_exhaustive()
        }
    }

    // /// Files whose answers carry a seal.
    // scope sealed {
    //     path "sealed";
    //     status { 2xx { body, headers } => unseal(body, headers)? }
    //     /// A file, as it comes, once the seal of its answer is checked.
    //     GET file(
    //         /// The identifier of the file.
    //         id: u64,
    //     ) -> concord::ByteStream {
    //         path "{id}";
    //         read Stream;
    //     }
    // }
    impl shapes::Sealed {
        /// A file, as it comes, once the seal of its answer is checked.
        ///
        /// Call: `shapes.sealed().file(id).await?`.
        ///
        /// Request: `GET https://shapes.example.com/sealed/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the file.
        ///
        /// Status: `2xx`.
        ///
        /// `.await` gives [`shapes::Result`]<[`concord::ByteStream`]>.
        pub fn file(&self, id: impl ::concord::arg::Arg<u64>) -> shapes::sealed::File {
            shapes::sealed::File {
                __scope: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }
    }

    impl ::std::fmt::Debug for shapes::Sealed {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("shapes::Sealed").finish_non_exhaustive()
        }
    }

    // /// Send an item again. This `POST` may be repeated.
    // POST again(
    //     /// The identifier of the item.
    //     id: u64,
    // ) -> Item {
    //     path "again/{id}";
    //     idempotent;
    // }

    impl shapes::Again {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::POST);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("again")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.again";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Item>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Item, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Item, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            // `idempotent;` alone is Yes.
            let __idempotent = ::concord::__private::Idempotent::Yes;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Again,
        Item,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Item`]>",
        awaitable,
        unconditional
    );

    impl ::std::fmt::Debug for shapes::Again {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Again")
                .field("endpoint", &"shapes.again")
                .finish_non_exhaustive()
        }
    }

    // /// Replace an item. This `PUT` may not be repeated.
    // PUT replace(
    //     /// The identifier of the item to replace.
    //     id: u64,
    //     /// The new item.
    //     item: Item,
    // ) -> Item {
    //     path "items/{id}";
    //     body json(item);
    //     idempotent false;
    // }

    impl shapes::Replace {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::PUT);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("items")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let id = &self.id;
                let item = &self.item;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            #[allow(unused_variables)]
            {
                let client = __Client;
                let id = &self.id;
                let item = &self.item;
                #[allow(unused_braces, clippy::redundant_closure_call)]
                let __body = (|| ::std::result::Result::Ok({ json(item) }))();
                let __body = match __body {
                    ::std::result::Result::Ok(__value) => __value,
                    ::std::result::Result::Err(__error) => {
                        return ::std::result::Result::Err(::concord::__private::build_error(
                            Self::__ENDPOINT,
                            __error,
                        ))
                    }
                };
                __draft.body(__body);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.replace";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Item>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Item, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Item, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            // docs/internals/macro.md requires the idempotent expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __idempotent = {
                let client = __Client;
                let id = &self.id;
                let item = &self.item;
                (|| ::concord::__private::idempotent(false))()
            };
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Replace,
        Item,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Item`]>",
        awaitable,
        unconditional
    );

    impl ::std::fmt::Debug for shapes::Replace {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Replace")
                .field("endpoint", &"shapes.replace")
                .finish_non_exhaustive()
        }
    }

    // /// Note when each expression of a send runs.
    // POST order(
    //     /// What each expression notes, and what it gives.
    //     trace: Trace,
    // ) -> Item {
    //     path "order";
    //     query { "step" = trace.note("query"); }
    //     idempotent trace.idempotent();
    //     idle_timeout trace.idle_timeout();
    // }

    impl shapes::Order {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::POST);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("order")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let trace = &self.trace;
                #[allow(unused_braces, clippy::redundant_closure_call)]
                let __entry = (|| ::std::result::Result::Ok({ trace.note("query") }))();
                let __entry = match __entry {
                    ::std::result::Result::Ok(__value) => __value,
                    ::std::result::Result::Err(__error) => {
                        return ::std::result::Result::Err(
                            ::concord::__private::Draft::entry_error(
                                Self::__ENDPOINT,
                                "step",
                                __error,
                            ),
                        )
                    }
                };
                __draft.query(
                    "step",
                    &[::concord::__private::Piece::Val(
                        "step",
                        &(&::concord::__private::ValueRef(&__entry)).adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.order";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Item>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Item, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            mut self,
        ) -> shapes::Result<::concord::__private::Prepared<Item, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            // For each send: `idle_timeout` first of all, then `idempotent`, then the request.
            // Each runs inside a non-async closure, where `?` and `.await` do not compile.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __idle_timeout = {
                let client = __Client;
                let trace = &self.trace;
                (|| -> ::std::time::Duration { trace.idle_timeout() })()
            };
            self.__options.description_idle_timeout(__idle_timeout);
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __idempotent = {
                let client = __Client;
                let trace = &self.trace;
                (|| ::concord::__private::idempotent(trace.idempotent()))()
            };
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Order,
        Item,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Item`]>",
        awaitable,
        unconditional
    );

    impl ::std::fmt::Debug for shapes::Order {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Order")
                .field("endpoint", &"shapes.order")
                .finish_non_exhaustive()
        }
    }

    // /// Upload a body that the caller gives: bytes, or a stream.
    // POST upload(
    //     /// The body to send.
    //     body: concord::Body,
    // ) -> Item {
    //     path "upload";
    //     body body;
    // }

    impl shapes::Upload {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::POST);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("upload")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let body = &self.body;
                #[allow(unused_braces, clippy::redundant_closure_call)]
                let __body = (|| ::std::result::Result::Ok({ body }))();
                let __body = match __body {
                    ::std::result::Result::Ok(__value) => __value,
                    ::std::result::Result::Err(__error) => {
                        return ::std::result::Result::Err(::concord::__private::build_error(
                            Self::__ENDPOINT,
                            __error,
                        ))
                    }
                };
                __draft.body(__body);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.upload";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Item>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Item, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Item, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Upload,
        Item,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Item`]>",
        awaitable,
        unconditional
    );

    impl ::std::fmt::Debug for shapes::Upload {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Upload")
                .field("endpoint", &"shapes.upload")
                .finish_non_exhaustive()
        }
    }

    // /// An item, or the conflict that holds it.
    // GET outcome(
    //     /// The identifier of the item.
    //     id: u64,
    // ) -> Result<Item, Conflict> {
    //     path "outcome/{id}";
    //     status { 409 { body } => Err(body) }
    // }

    impl shapes::Outcome {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("outcome")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.outcome";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Result<Item, Conflict>>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                ::concord::http::StatusCode::CONFLICT => {
                    let __body = ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?;
                    // The arm of the endpoint: the owner in reach, then its inputs.
                    let client = __Client;
                    let id = &self.id;
                    #[allow(unused_braces)]
                    let __arm = |body| -> ::std::result::Result<
                        Result<Item, Conflict>,
                        ::concord::Fail<ShapesProblem>,
                    > { ::std::result::Result::Ok({ Err(body) }) };
                    ::concord::__private::arm(__arm(__body), __status, &__headers, &__info)?
                }
                __status if __status.is_success() => ::std::result::Result::Ok(
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Item, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?,
                ),
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Result<Item, Conflict>, ShapesProblem>>
        {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Outcome,
        Result<Item, Conflict>,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Result`]<[`Item`], [`Conflict`]>>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::Outcome {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Outcome")
                .field("endpoint", &"shapes.outcome")
                .finish_non_exhaustive()
        }
    }

    // /// Check that the service answers.
    // HEAD ping() {
    //     path "ping";
    // }

    impl shapes::Ping {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::HEAD);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("ping")]);
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.ping";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<()>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            match __status {
                __status if __status.is_success() => (),
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, ()).with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<(), ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Ping,
        (),
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::Ping {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Ping")
                .field("endpoint", &"shapes.ping")
                .finish_non_exhaustive()
        }
    }

    // /// Whether an item is present. Only a `204` says yes.
    // HEAD present(
    //     /// The identifier of the item.
    //     id: u64,
    // ) -> bool {
    //     path "items/{id}";
    //     status { 204 => true, 404 => false }
    // }

    impl shapes::Present {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::HEAD);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("items")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.present";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<bool>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                ::concord::http::StatusCode::NO_CONTENT => {
                    // The arm of the endpoint: the owner in reach, then its inputs.
                    let client = __Client;
                    let id = &self.id;
                    #[allow(unused_braces)]
                    let __arm =
                        || -> ::std::result::Result<bool, ::concord::Fail<ShapesProblem>> {
                            ::std::result::Result::Ok({ true })
                        };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                ::concord::http::StatusCode::NOT_FOUND => {
                    // The arm of the endpoint: the owner in reach, then its inputs.
                    let client = __Client;
                    let id = &self.id;
                    #[allow(unused_braces)]
                    let __arm =
                        || -> ::std::result::Result<bool, ::concord::Fail<ShapesProblem>> {
                            ::std::result::Result::Ok({ false })
                        };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                __status if __status.is_success() => {
                    return ::core::result::Result::Err(::concord::__private::no_arm(__response, &__info).await)
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<bool, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Present,
        bool,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`bool`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::Present {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Present")
                .field("endpoint", &"shapes.present")
                .finish_non_exhaustive()
        }
    }

    // /// Create an item, and keep where it was put.
    // POST create(
    //     /// The item to create.
    //     item: Item,
    // ) -> Receipt {
    //     path "items";
    //     body json(item);
    //     status {
    //         201 { headers, status } => Receipt {
    //             status: status.as_u16(),
    //             location: Some(headers.require("location")?.to_owned()),
    //         },
    //         202 { status: code } => Receipt { status: code.as_u16(), location: None },
    //         3xx => Err(concord::mismatch("a redirect is not a receipt"))?,
    //     }
    // }

    impl shapes::Create {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::POST);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("items")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let item = &self.item;
                #[allow(unused_braces, clippy::redundant_closure_call)]
                let __body = (|| ::std::result::Result::Ok({ json(item) }))();
                let __body = match __body {
                    ::std::result::Result::Ok(__value) => __value,
                    ::std::result::Result::Err(__error) => {
                        return ::std::result::Result::Err(::concord::__private::build_error(
                            Self::__ENDPOINT,
                            __error,
                        ))
                    }
                };
                __draft.body(__body);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.create";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Receipt>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                ::concord::http::StatusCode::CREATED => {
                    // The arm of the endpoint: the owner in reach, then its inputs.
                    let client = __Client;
                    let item = &self.item;
                    #[allow(unused_braces)]
                    let __arm = |headers: &::concord::Headers,
                                 status: ::concord::http::StatusCode|
                     -> ::std::result::Result<
                        Receipt,
                        ::concord::Fail<ShapesProblem>,
                    > {
                        ::std::result::Result::Ok({
                            Receipt {
                                status: status.as_u16(),
                                location: Some(headers.require("location")?.to_owned()),
                            }
                        })
                    };
                    ::concord::__private::arm(
                        __arm(&__headers, __status),
                        __status,
                        &__headers,
                        &__info,
                    )?
                }
                ::concord::http::StatusCode::ACCEPTED => {
                    // The arm of the endpoint: the owner in reach, then its inputs.
                    let client = __Client;
                    let item = &self.item;
                    #[allow(unused_braces)]
                    let __arm = |code: ::concord::http::StatusCode| -> ::std::result::Result<
                        Receipt,
                        ::concord::Fail<ShapesProblem>,
                    > {
                        ::std::result::Result::Ok({
                            Receipt {
                                status: code.as_u16(),
                                location: None,
                            }
                        })
                    };
                    ::concord::__private::arm(__arm(__status), __status, &__headers, &__info)?
                }
                __status if __status.is_redirection() => {
                    // The arm of the endpoint: the owner in reach, then its inputs.
                    let client = __Client;
                    let item = &self.item;
                    #[allow(unused_braces)]
                    let __arm =
                        || -> ::std::result::Result<Receipt, ::concord::Fail<ShapesProblem>> {
                            ::std::result::Result::Ok({
                                Err(concord::mismatch("a redirect is not a receipt"))?
                            })
                        };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Receipt, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Receipt, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Create,
        Receipt,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Receipt`]>",
        awaitable,
        unconditional
    );

    impl ::std::fmt::Debug for shapes::Create {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Create")
                .field("endpoint", &"shapes.create")
                .finish_non_exhaustive()
        }
    }

    // /// An item, on the host of a tenant.
    // GET hosted(
    //     /// The tenant: one label of the host.
    //     tenant: String,
    // ) -> Item {
    //     base "https://{tenant}.shapes.example.com";
    //     path "tenant";
    // }

    impl shapes::Hosted {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let tenant = &self.tenant;
                __draft.base(&[
                    ::concord::__private::Piece::Lit("https://"),
                    ::concord::__private::Piece::Val(
                        "tenant",
                        &(&::concord::__private::ValueRef(tenant)).adapt(),
                    ),
                    ::concord::__private::Piece::Lit(".shapes.example.com"),
                ]);
            }
            __draft.segment(&[::concord::__private::Piece::Lit("tenant")]);
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.hosted";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Item>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Item, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Item, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Hosted,
        Item,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Item`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::Hosted {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Hosted")
                .field("endpoint", &"shapes.hosted")
                .finish_non_exhaustive()
        }
    }

    // /// Search with a number, and a filter that a query cannot hold.
    // GET search(
    //     /// A number, written in the query as JSON writes it.
    //     ratio: f64,
    //     /// A struct: a query refuses it when the request is built.
    //     filter?: Filter,
    // ) -> Vec<Item> {
    //     path "search";
    //     query { ratio; filter; }
    // }
    /// The options of this endpoint.
    impl shapes::Search {
        /// A struct: a query refuses it when the request is built.
        ///
        /// Accepts [`Filter`], &[`Filter`], or an [`Option`]<[`Filter`]>. `None` clears it.
        pub fn filter(mut self, filter: impl ::concord::arg::Arg<::core::option::Option<Filter>>) -> Self {
            self.filter = filter.into_input();
            self
        }
    }

    impl shapes::Search {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("search")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let ratio = &self.ratio;
                let filter = &self.filter;
                __draft.query(
                    "ratio",
                    &[::concord::__private::Piece::Val(
                        "ratio",
                        &(&::concord::__private::ValueRef(ratio)).adapt(),
                    )],
                );
            }
            #[allow(unused_variables)]
            {
                let client = __Client;
                let ratio = &self.ratio;
                let filter = &self.filter;
                __draft.query(
                    "filter",
                    &[::concord::__private::Piece::Val(
                        "filter",
                        &(&::concord::__private::ValueRef(filter)).adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.search";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Vec<Item>>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Vec<Item>, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Vec<Item>, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Search,
        Vec<Item>,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Vec`]<[`Item`]>>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::Search {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Search")
                .field("endpoint", &"shapes.search")
                .finish_non_exhaustive()
        }
    }

    // /// Send an item with a length written by hand, in a body that may fail.
    // POST measured(
    //     /// The item. One with an empty title is not written.
    //     item: Item,
    //     /// The content-length to send.
    //     length: u64,
    // ) -> Item {
    //     path "measured";
    //     headers { "content-length" = length; }
    //     body checked(item);
    // }

    impl shapes::Measured {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::POST);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("measured")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let item = &self.item;
                let length = &self.length;
                __draft.header(
                    "content-length",
                    &[::concord::__private::Piece::Val(
                        "content-length",
                        &(&::concord::__private::ValueRef(length)).adapt(),
                    )],
                );
            }
            #[allow(unused_variables)]
            {
                let client = __Client;
                let item = &self.item;
                let length = &self.length;
                #[allow(unused_braces, clippy::redundant_closure_call)]
                let __body = (|| ::std::result::Result::Ok({ checked(item) }))();
                let __body = match __body {
                    ::std::result::Result::Ok(__value) => __value,
                    ::std::result::Result::Err(__error) => {
                        return ::std::result::Result::Err(::concord::__private::build_error(
                            Self::__ENDPOINT,
                            __error,
                        ))
                    }
                };
                __draft.body(__body);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.measured";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Item>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Item, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Item, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Measured,
        Item,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Item`]>",
        awaitable,
        unconditional
    );

    impl ::std::fmt::Debug for shapes::Measured {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Measured")
                .field("endpoint", &"shapes.measured")
                .finish_non_exhaustive()
        }
    }

    // /// An item, asked with a number that is parsed when the request is built.
    // GET parsed(
    //     /// The number, as text.
    //     raw: String,
    // ) -> Item {
    //     path "parsed";
    //     query { "n" = raw.parse::<u32>()?; }
    // }

    impl shapes::Parsed {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("parsed")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let raw = &self.raw;
                #[allow(unused_braces, clippy::redundant_closure_call)]
                let __entry = (|| ::std::result::Result::Ok({ raw.parse::<u32>()? }))();
                let __entry = match __entry {
                    ::std::result::Result::Ok(__value) => __value,
                    ::std::result::Result::Err(__error) => {
                        return ::std::result::Result::Err(
                            ::concord::__private::Draft::entry_error(
                                Self::__ENDPOINT,
                                "n",
                                __error,
                            ),
                        )
                    }
                };
                __draft.query(
                    "n",
                    &[::concord::__private::Piece::Val(
                        "n",
                        &(&::concord::__private::ValueRef(&__entry)).adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.parsed";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Item>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Item, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Item, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Parsed,
        Item,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Item`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::Parsed {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Parsed")
                .field("endpoint", &"shapes.parsed")
                .finish_non_exhaustive()
        }
    }

    // /// Counts by name.
    // GET counts() -> Tally {
    //     path "counts";
    // }

    impl shapes::Counts {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("counts")]);
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.counts";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Tally>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Tally, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Tally, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Counts,
        Tally,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Tally`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::Counts {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Counts")
                .field("endpoint", &"shapes.counts")
                .finish_non_exhaustive()
        }
    }

    // /// An item, read only when the answer is labelled JSON.
    // GET strict() -> Item {
    //     path "strict";
    //     read Json.strict();
    // }

    impl shapes::Strict {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("strict")]);
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.strict";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Item>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Item, _>(Json.strict()))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Item, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Strict,
        Item,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Item`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::Strict {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Strict")
                .field("endpoint", &"shapes.strict")
                .finish_non_exhaustive()
        }
    }

    // /// An item, read by a reader whose message is long.
    // GET long_message() -> Item {
    //     path "long-message";
    //     read Chatty;
    // }

    impl shapes::LongMessage {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("long-message")]);
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.long_message";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Item>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Item, _>(Chatty))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Item, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::LongMessage,
        Item,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Item`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::LongMessage {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("LongMessage")
                .field("endpoint", &"shapes.long_message")
                .finish_non_exhaustive()
        }
    }

    // /// An item, out of its envelope.
    // GET get(
    //     /// The identifier of the item.
    //     id: u64,
    // ) -> Item {
    //     path "{id}";
    // }

    impl shapes::wrapped::Get {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("wrapped")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let wrapped = __Wrapped;
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(
                    &self.__scope.__client.__core,
                    &self.__options,
                    Self::__ENDPOINT,
                )
            } else {
                __draft.finish(
                    &self.__scope.__client.__core,
                    &self.__options,
                    Self::__ENDPOINT,
                )
            }
        }

        const __ENDPOINT: &'static str = "shapes.wrapped.get";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Item>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                // The arm of the scope: it takes every 2xx.
                __status if __status.is_success() => {
                    let __body = ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?;
                    // The arm of the scope: the owners in reach; the scope has no input.
                    let client = __Client;
                    let wrapped = __Wrapped;
                    #[allow(unused_braces)]
                    let __arm =
                        |body| -> ::std::result::Result<Item, ::concord::Fail<ShapesProblem>> {
                            ::std::result::Result::Ok({ unpack(body)? })
                        };
                    ::concord::__private::arm(__arm(__body), __status, &__headers, &__info)?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Item, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__scope.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__scope.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::wrapped::Get,
        Item,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Item`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::wrapped::Get {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Get")
                .field("endpoint", &"shapes.wrapped.get")
                .finish_non_exhaustive()
        }
    }

    // /// The envelope itself.
    // GET raw(
    //     /// The identifier of the item.
    //     id: u64,
    // ) -> Envelope<Item> {
    //     path "raw/{id}";
    //     status { remove 2xx }
    // }

    impl shapes::wrapped::Raw {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("wrapped")]);
            __draft.segment(&[::concord::__private::Piece::Lit("raw")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let wrapped = __Wrapped;
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(
                    &self.__scope.__client.__core,
                    &self.__options,
                    Self::__ENDPOINT,
                )
            } else {
                __draft.finish(
                    &self.__scope.__client.__core,
                    &self.__options,
                    Self::__ENDPOINT,
                )
            }
        }

        const __ENDPOINT: &'static str = "shapes.wrapped.raw";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Envelope<Item>>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                // `remove 2xx`: the arm of the scope is taken away, and the simple rule reads.
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Envelope<Item>, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Envelope<Item>, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__scope.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__scope.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::wrapped::Raw,
        Envelope<Item>,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Envelope`]<[`Item`]>>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::wrapped::Raw {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Raw")
                .field("endpoint", &"shapes.wrapped.raw")
                .finish_non_exhaustive()
        }
    }

    // /// An item out of its envelope, or none.
    // GET find(
    //     /// The identifier of the item.
    //     id: u64,
    // ) -> Option<Item> {
    //     path "find/{id}";
    //     status { override 2xx { body } => Some(unpack(body)?), 404 => None }
    // }

    impl shapes::wrapped::Find {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("wrapped")]);
            __draft.segment(&[::concord::__private::Piece::Lit("find")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let wrapped = __Wrapped;
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(
                    &self.__scope.__client.__core,
                    &self.__options,
                    Self::__ENDPOINT,
                )
            } else {
                __draft.finish(
                    &self.__scope.__client.__core,
                    &self.__options,
                    Self::__ENDPOINT,
                )
            }
        }

        const __ENDPOINT: &'static str = "shapes.wrapped.find";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Option<Item>>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                ::concord::http::StatusCode::NOT_FOUND => {
                    // The arm of the endpoint: the owners in reach, then its inputs.
                    let client = __Client;
                    let wrapped = __Wrapped;
                    let id = &self.id;
                    #[allow(unused_braces)]
                    let __arm = || -> ::std::result::Result<
                        Option<Item>,
                        ::concord::Fail<ShapesProblem>,
                    > { ::std::result::Result::Ok({ None }) };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                // `override 2xx`: the endpoint's own arm, in place of the scope's.
                __status if __status.is_success() => {
                    let __body = ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?;
                    // The arm of the endpoint: the owners in reach, then its inputs.
                    let client = __Client;
                    let wrapped = __Wrapped;
                    let id = &self.id;
                    #[allow(unused_braces)]
                    let __arm = |body| -> ::std::result::Result<
                        Option<Item>,
                        ::concord::Fail<ShapesProblem>,
                    > {
                        ::std::result::Result::Ok({ Some(unpack(body)?) })
                    };
                    ::concord::__private::arm(__arm(__body), __status, &__headers, &__info)?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Option<Item>, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__scope.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__scope.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::wrapped::Find,
        Option<Item>,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Option`]<[`Item`]>>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::wrapped::Find {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Find")
                .field("endpoint", &"shapes.wrapped.find")
                .finish_non_exhaustive()
        }
    }

    // /// Touch an item. An accepted touch is not done; a missing item is fine.
    // HEAD touch(
    //     /// The identifier of the item.
    //     id: u64,
    // ) {
    //     path "touch/{id}";
    //     status {
    //         override 202 => Err(concord::mismatch("an accepted touch is not done"))?,
    //         404 => (),
    //     }
    // }
    impl shapes::wrapped::Touch {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::HEAD);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("wrapped")]);
            __draft.segment(&[::concord::__private::Piece::Lit("touch")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let wrapped = __Wrapped;
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(
                    &self.__scope.__client.__core,
                    &self.__options,
                    Self::__ENDPOINT,
                )
            } else {
                __draft.finish(
                    &self.__scope.__client.__core,
                    &self.__options,
                    Self::__ENDPOINT,
                )
            }
        }

        const __ENDPOINT: &'static str = "shapes.wrapped.touch";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<()>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            match __status {
                ::concord::http::StatusCode::ACCEPTED => {
                    // `override 202`: the endpoint's own arm; the owners in reach, then its inputs.
                    let client = __Client;
                    let wrapped = __Wrapped;
                    let id = &self.id;
                    // An arm under no arrow gives `()`: the author's expression is copied as written.
                    #[allow(unused_braces, clippy::unit_arg, clippy::unused_unit)]
                    let __arm = || -> ::std::result::Result<(), ::concord::Fail<ShapesProblem>> {
                        ::std::result::Result::Ok({ Err(concord::mismatch("an accepted touch is not done"))? })
                    };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                ::concord::http::StatusCode::NOT_FOUND => {
                    // The arm of the endpoint: the owners in reach, then its inputs.
                    let client = __Client;
                    let wrapped = __Wrapped;
                    let id = &self.id;
                    // An arm under no arrow gives `()`: the author's expression is copied as written.
                    #[allow(unused_braces, clippy::unit_arg, clippy::unused_unit)]
                    let __arm = || -> ::std::result::Result<(), ::concord::Fail<ShapesProblem>> {
                        ::std::result::Result::Ok({ () })
                    };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                // A `HEAD`: the scope's arm binds `body`, and is left out (C5.3).
                __status if __status.is_success() => (),
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, ()).with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<(), ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__scope.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__scope.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::wrapped::Touch,
        (),
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::wrapped::Touch {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Touch")
                .field("endpoint", &"shapes.wrapped.touch")
                .finish_non_exhaustive()
        }
    }

    // /// Whether an item is held. Only a `204` says yes.
    // HEAD holds(
    //     /// The identifier of the item.
    //     id: u64,
    // ) -> bool {
    //     path "holds/{id}";
    //     status { override 204 => true, 404 => false }
    // }
    impl shapes::wrapped::Holds {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::HEAD);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("wrapped")]);
            __draft.segment(&[::concord::__private::Piece::Lit("holds")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let wrapped = __Wrapped;
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(
                    &self.__scope.__client.__core,
                    &self.__options,
                    Self::__ENDPOINT,
                )
            } else {
                __draft.finish(
                    &self.__scope.__client.__core,
                    &self.__options,
                    Self::__ENDPOINT,
                )
            }
        }

        const __ENDPOINT: &'static str = "shapes.wrapped.holds";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<bool>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                ::concord::http::StatusCode::NO_CONTENT => {
                    // `override 204`: the endpoint's own arm; the owners in reach, then its inputs.
                    let client = __Client;
                    let wrapped = __Wrapped;
                    let id = &self.id;
                    #[allow(unused_braces)]
                    let __arm = || -> ::std::result::Result<bool, ::concord::Fail<ShapesProblem>> {
                        ::std::result::Result::Ok({ true })
                    };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                ::concord::http::StatusCode::NOT_FOUND => {
                    // The arm of the endpoint: the owners in reach, then its inputs.
                    let client = __Client;
                    let wrapped = __Wrapped;
                    let id = &self.id;
                    #[allow(unused_braces)]
                    let __arm = || -> ::std::result::Result<bool, ::concord::Fail<ShapesProblem>> {
                        ::std::result::Result::Ok({ false })
                    };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                // A `HEAD`: the scope's arm binds `body`, and is left out (C5.3). No arm
                // takes this 2xx.
                __status if __status.is_success() => {
                    return ::core::result::Result::Err(::concord::__private::no_arm(__response, &__info).await)
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<bool, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__scope.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__scope.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::wrapped::Holds,
        bool,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`bool`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::wrapped::Holds {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Holds")
                .field("endpoint", &"shapes.wrapped.holds")
                .finish_non_exhaustive()
        }
    }

    // /// An item, read by a reader that notes each answer it reads.
    // GET noted(
    //     /// What the reader notes.
    //     trace: Trace,
    // ) -> Item {
    //     path "noted";
    //     read { trace.note("read"); Json };
    // }

    impl shapes::Noted {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("noted")]);
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.noted";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Item>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        // The endpoint's `read`: the owner in reach, then its inputs.
                        &{
                            let client = __Client;
                            let trace = &self.trace;
                            (|| {
                                ::concord::__private::reader::<Item, _>({
                                    trace.note("read");
                                    Json
                                })
                            })()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        // The client's reader of its problem: the client's bindings only.
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Item, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Noted,
        Item,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Item`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::Noted {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Noted")
                .field("endpoint", &"shapes.noted")
                .finish_non_exhaustive()
        }
    }

    // /// An item; a missing one is made of what was asked.
    // GET titled(
    //     /// The identifier of the item.
    //     id: u64,
    //     /// The title of a missing item.
    //     title: String,
    // ) -> Item {
    //     path "titled/{id}";
    //     status { 404 => Item { id: *id, title: title.clone() } }
    // }

    impl shapes::Titled {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("titled")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let id = &self.id;
                let title = &self.title;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.titled";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Item>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                ::concord::http::StatusCode::NOT_FOUND => {
                    // The arm of the endpoint: the owner in reach, then its inputs.
                    let client = __Client;
                    let id = &self.id;
                    let title = &self.title;
                    #[allow(unused_braces)]
                    let __arm =
                        || -> ::std::result::Result<Item, ::concord::Fail<ShapesProblem>> {
                            ::std::result::Result::Ok({
                                Item {
                                    id: *id,
                                    title: title.clone(),
                                }
                            })
                        };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Item, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Item, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Titled,
        Item,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Item`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::Titled {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Titled")
                .field("endpoint", &"shapes.titled")
                .finish_non_exhaustive()
        }
    }

    // /// An item, or the conflict that holds it, asked by an input named `body`.
    // GET held(
    //     /// The identifier of the item. Its name is the field of an arm.
    //     body: u64,
    // ) -> Result<Item, Conflict> {
    //     path "held/{body}";
    //     status {
    //         409 { body: answer } => {
    //             let conflict: Conflict = answer;
    //             Err(Conflict { message: format!("item {body}: {}", conflict.message) })
    //         }
    //     }
    // }

    impl shapes::Held {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("held")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let body = &self.body;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "body",
                    &(&::concord::__private::ValueRef(body)).adapt(),
                )]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.held";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Result<Item, Conflict>>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                ::concord::http::StatusCode::CONFLICT => {
                    // The value that was read is the expansion's own: `__body`.
                    let __body = ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?;
                    // The arm of the endpoint: the owner in reach, then its inputs; then
                    // the arm's own field, `body` renamed `answer`, beside the input `body`.
                    let client = __Client;
                    let body = &self.body;
                    #[allow(unused_braces)]
                    let __arm = |answer| -> ::std::result::Result<
                        Result<Item, Conflict>,
                        ::concord::Fail<ShapesProblem>,
                    > {
                        ::std::result::Result::Ok({
                            let conflict: Conflict = answer;
                            Err(Conflict {
                                message: format!("item {body}: {}", conflict.message),
                            })
                        })
                    };
                    ::concord::__private::arm(__arm(__body), __status, &__headers, &__info)?
                }
                __status if __status.is_success() => ::std::result::Result::Ok(
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Item, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?,
                ),
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Result<Item, Conflict>, ShapesProblem>>
        {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Held,
        Result<Item, Conflict>,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Result`]<[`Item`], [`Conflict`]>>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::Held {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Held")
                .field("endpoint", &"shapes.held")
                .finish_non_exhaustive()
        }
    }

    // /// An item. A refusal is read by the arm of the endpoint, and ends with its problem.
    // GET refusal(
    //     /// The identifier of the item.
    //     id: u64,
    // ) -> Item {
    //     path "refusal/{id}";
    //     status { 4xx { body } => Err::<_, ShapesProblem>(body)? }
    // }

    impl shapes::Refusal {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("refusal")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let id = &self.id;
                __draft.segment(&[
                    ::concord::__private::Piece::Val(
                        "id",
                        &(&::concord::__private::ValueRef(id)).adapt(),
                    ),
                ]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.refusal";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Item>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Item, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                __status if __status.is_client_error() => {
                    let __body = ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?;
                    // The arm of the endpoint: the owner in reach, then its inputs.
                    let client = __Client;
                    let id = &self.id;
                    #[allow(unused_braces)]
                    let __arm = |body| -> ::std::result::Result<
                        Item,
                        ::concord::Fail<ShapesProblem>,
                    > { ::std::result::Result::Ok({ Err::<_, ShapesProblem>(body)? }) };
                    ::concord::__private::arm(__arm(__body), __status, &__headers, &__info)?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Item, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Refusal,
        Item,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Item`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::Refusal {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Refusal")
                .field("endpoint", &"shapes.refusal")
                .finish_non_exhaustive()
        }
    }

    // /// Pages of items behind three keys: one in the path, one in the query, one in a header.
    // GET guarded(
    //     /// A key written in the path.
    //     path_key: Secret,
    //     /// A key written as the query pair `key`.
    //     query_key?: Secret,
    //     /// A key written in the header `x-key`.
    //     header_key?: Secret,
    //     /// The cursor of the page.
    //     cursor?: String,
    // ) -> Batch {
    //     path "guarded/{path_key}";
    //     query { "key" = query_key; cursor; }
    //     headers { "x-key" = header_key; }
    //     paginate next_batch(cursor);
    // }

    impl ::concord::page::Slot<shapes::Guarded> for shapes::__GuardedPathKey {
        type Value = Secret;
        fn get<'a>(&self, __call: &'a shapes::Guarded) -> ::core::option::Option<&'a Self::Value> {
            ::core::option::Option::Some(&__call.path_key)
        }
        fn set(&self, mut __call: shapes::Guarded, __value: Self::Value) -> shapes::Guarded {
            __call.path_key = __value;
            __call
        }
    }

    impl ::concord::page::Slot<shapes::Guarded> for shapes::__GuardedQueryKey {
        type Value = Secret;
        fn get<'a>(&self, __call: &'a shapes::Guarded) -> ::core::option::Option<&'a Self::Value> {
            __call.query_key.as_ref()
        }
        fn set(&self, mut __call: shapes::Guarded, __value: Self::Value) -> shapes::Guarded {
            __call.query_key = ::core::option::Option::Some(__value);
            __call
        }
    }

    impl ::concord::page::Slot<shapes::Guarded> for shapes::__GuardedHeaderKey {
        type Value = Secret;
        fn get<'a>(&self, __call: &'a shapes::Guarded) -> ::core::option::Option<&'a Self::Value> {
            __call.header_key.as_ref()
        }
        fn set(&self, mut __call: shapes::Guarded, __value: Self::Value) -> shapes::Guarded {
            __call.header_key = ::core::option::Option::Some(__value);
            __call
        }
    }

    impl ::concord::page::Slot<shapes::Guarded> for shapes::__GuardedCursor {
        type Value = String;
        fn get<'a>(&self, __call: &'a shapes::Guarded) -> ::core::option::Option<&'a Self::Value> {
            __call.cursor.as_ref()
        }
        fn set(&self, mut __call: shapes::Guarded, __value: Self::Value) -> shapes::Guarded {
            __call.cursor = ::core::option::Option::Some(__value);
            __call
        }
    }

    /// The options of this endpoint.
    impl shapes::Guarded {
        /// A key written as the query pair `key`.
        ///
        /// Accepts [`Secret`], &[`Secret`], [`String`], &[`str`], or an [`Option`]<[`Secret`]>. `None` clears it.
        pub fn query_key(mut self, query_key: impl ::concord::arg::Arg<::core::option::Option<Secret>>) -> Self {
            self.query_key = query_key.into_input();
            self
        }

        /// A key written in the header `x-key`.
        ///
        /// Accepts [`Secret`], &[`Secret`], [`String`], &[`str`], or an [`Option`]<[`Secret`]>. `None` clears it.
        pub fn header_key(mut self, header_key: impl ::concord::arg::Arg<::core::option::Option<Secret>>) -> Self {
            self.header_key = header_key.into_input();
            self
        }

        /// The cursor of the page.
        ///
        /// Accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it.
        pub fn cursor(mut self, cursor: impl ::concord::arg::Arg<::core::option::Option<String>>) -> Self {
            self.cursor = cursor.into_input();
            self
        }
    }

    impl shapes::Guarded {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("guarded")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let path_key = &self.path_key;
                let query_key = &self.query_key;
                let header_key = &self.header_key;
                let cursor = &self.cursor;
                __draft.segment(&[
                    ::concord::__private::Piece::Val(
                        "path_key",
                        &(&::concord::__private::ValueRef(path_key)).adapt(),
                    ),
                ]);
            }
            #[allow(unused_variables)]
            {
                let client = __Client;
                let path_key = &self.path_key;
                let query_key = &self.query_key;
                let header_key = &self.header_key;
                let cursor = &self.cursor;
                __draft.query(
                    "key",
                    &[
                        ::concord::__private::Piece::Val(
                            "key",
                            &(&::concord::__private::ValueRef(query_key)).adapt(),
                        ),
                    ],
                );
            }
            #[allow(unused_variables)]
            {
                let client = __Client;
                let path_key = &self.path_key;
                let query_key = &self.query_key;
                let header_key = &self.header_key;
                let cursor = &self.cursor;
                __draft.query(
                    "cursor",
                    &[
                        ::concord::__private::Piece::Val(
                            "cursor",
                            &(&::concord::__private::ValueRef(cursor)).adapt(),
                        ),
                    ],
                );
            }
            #[allow(unused_variables)]
            {
                let client = __Client;
                let path_key = &self.path_key;
                let query_key = &self.query_key;
                let header_key = &self.header_key;
                let cursor = &self.cursor;
                __draft.header(
                    "x-key",
                    &[
                        ::concord::__private::Piece::Val(
                            "x-key",
                            &(&::concord::__private::ValueRef(header_key)).adapt(),
                        ),
                    ],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.guarded";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Batch>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Batch, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Batch, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Guarded,
        Batch,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Batch`]>",
        paged, conditional,
        pages {
            /// Iterate over [`Batch`] pages.
            ///
            /// `try_next().await` gives [`shapes::Result`]<[`Option`]<[`Batch`]>>.
            /// Each page is one call. Nothing is asked until iteration starts.
            pub fn pages(self) -> ::concord::Pages<Batch, ShapesProblem> {
                // The owner in reach, then each input as its place; the expression is
                // copied as written.
                ::concord::__private::pages(self, {
                    #[allow(unused_variables)]
                    let client = __Client;
                    #[allow(unused_variables)]
                    let (path_key, query_key, header_key, cursor) = (
                        shapes::__GuardedPathKey,
                        shapes::__GuardedQueryKey,
                        shapes::__GuardedHeaderKey,
                        shapes::__GuardedCursor,
                    );
                    next_batch(cursor)
                })
            }

        }
    );

    impl ::std::fmt::Debug for shapes::Guarded {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Guarded")
                .field("endpoint", &"shapes.guarded")
                .finish_non_exhaustive()
        }
    }

    // /// Pages of items whose cursor goes in a header.
    // GET headed(
    //     /// The cursor of the page, sent as `x-cursor`.
    //     cursor?: String,
    // ) -> Batch {
    //     path "headed";
    //     headers { "x-cursor" = cursor; }
    //     paginate next_batch(cursor);
    // }

    impl ::concord::page::Slot<shapes::Headed> for shapes::__HeadedCursor {
        type Value = String;
        fn get<'a>(&self, __call: &'a shapes::Headed) -> ::core::option::Option<&'a Self::Value> {
            __call.cursor.as_ref()
        }
        fn set(&self, mut __call: shapes::Headed, __value: Self::Value) -> shapes::Headed {
            __call.cursor = ::core::option::Option::Some(__value);
            __call
        }
    }

    /// The options of this endpoint.
    impl shapes::Headed {
        /// The cursor of the page, sent as `x-cursor`.
        ///
        /// Accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it.
        pub fn cursor(mut self, cursor: impl ::concord::arg::Arg<::core::option::Option<String>>) -> Self {
            self.cursor = cursor.into_input();
            self
        }
    }

    impl shapes::Headed {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("headed")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let cursor = &self.cursor;
                __draft.header(
                    "x-cursor",
                    &[
                        ::concord::__private::Piece::Val(
                            "x-cursor",
                            &(&::concord::__private::ValueRef(cursor)).adapt(),
                        ),
                    ],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.headed";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Batch>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Batch, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Batch, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Headed,
        Batch,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Batch`]>",
        paged, conditional,
        pages {
            /// Iterate over [`Batch`] pages.
            ///
            /// `try_next().await` gives [`shapes::Result`]<[`Option`]<[`Batch`]>>.
            /// Each page is one call. Nothing is asked until iteration starts.
            pub fn pages(self) -> ::concord::Pages<Batch, ShapesProblem> {
                // The owner in reach, then each input as its place; the expression is
                // copied as written.
                ::concord::__private::pages(self, {
                    #[allow(unused_variables)]
                    let client = __Client;
                    #[allow(unused_variables)]
                    let cursor = shapes::__HeadedCursor;
                    next_batch(cursor)
                })
            }

        }
    );

    impl ::std::fmt::Debug for shapes::Headed {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Headed")
                .field("endpoint", &"shapes.headed")
                .finish_non_exhaustive()
        }
    }

    // /// Pages of items asked with a body of entries: a token and the cursor.
    // POST posted(
    //     /// A token written in the body.
    //     token?: Secret,
    //     /// The cursor of the page, written in the body.
    //     cursor?: String,
    // ) -> Batch {
    //     path "posted";
    //     body json { token; cursor; }
    //     paginate next_batch(cursor);
    // }

    impl ::concord::page::Slot<shapes::Posted> for shapes::__PostedToken {
        type Value = Secret;
        fn get<'a>(&self, __call: &'a shapes::Posted) -> ::core::option::Option<&'a Self::Value> {
            __call.token.as_ref()
        }
        fn set(&self, mut __call: shapes::Posted, __value: Self::Value) -> shapes::Posted {
            __call.token = ::core::option::Option::Some(__value);
            __call
        }
    }

    impl ::concord::page::Slot<shapes::Posted> for shapes::__PostedCursor {
        type Value = String;
        fn get<'a>(&self, __call: &'a shapes::Posted) -> ::core::option::Option<&'a Self::Value> {
            __call.cursor.as_ref()
        }
        fn set(&self, mut __call: shapes::Posted, __value: Self::Value) -> shapes::Posted {
            __call.cursor = ::core::option::Option::Some(__value);
            __call
        }
    }

    /// The options of this endpoint.
    impl shapes::Posted {
        /// A token written in the body.
        ///
        /// Accepts [`Secret`], &[`Secret`], [`String`], &[`str`], or an [`Option`]<[`Secret`]>. `None` clears it.
        pub fn token(mut self, token: impl ::concord::arg::Arg<::core::option::Option<Secret>>) -> Self {
            self.token = token.into_input();
            self
        }

        /// The cursor of the page, written in the body.
        ///
        /// Accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it.
        pub fn cursor(mut self, cursor: impl ::concord::arg::Arg<::core::option::Option<String>>) -> Self {
            self.cursor = cursor.into_input();
            self
        }
    }

    impl shapes::Posted {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::POST);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("posted")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let token = &self.token;
                let cursor = &self.cursor;
                // Every entry uses the same adapter and insertion path.
                let __value0 = ::concord::__private::ValueRef(token);
                let __value1 = ::concord::__private::ValueRef(cursor);
                let mut __entries = ::concord::body::Entries::new();
                __entries.__push("token", (&__value0).adapt());
                __entries.__push("cursor", (&__value1).adapt());
                __draft.body(json(__entries));
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.posted";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Batch>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Batch, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Batch, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Posted,
        Batch,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Batch`]>",
        paged, unconditional,
        pages {
            /// Iterate over [`Batch`] pages.
            ///
            /// `try_next().await` gives [`shapes::Result`]<[`Option`]<[`Batch`]>>.
            /// Each page is one call. Nothing is asked until iteration starts.
            pub fn pages(self) -> ::concord::Pages<Batch, ShapesProblem> {
                // The owner in reach, then each input as its place; the expression is
                // copied as written.
                ::concord::__private::pages(self, {
                    #[allow(unused_variables)]
                    let client = __Client;
                    #[allow(unused_variables)]
                    let (token, cursor) = (
                        shapes::__PostedToken,
                        shapes::__PostedCursor,
                    );
                    next_batch(cursor)
                })
            }

        }
    );

    impl ::std::fmt::Debug for shapes::Posted {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Posted")
                .field("endpoint", &"shapes.posted")
                .finish_non_exhaustive()
        }
    }

    // /// Pages of items asked with a body that the caller gives: bytes, or a stream.
    // POST streamed(
    //     /// The body of every page.
    //     file: concord::Body,
    //     /// The cursor of the page.
    //     cursor?: String,
    // ) -> Batch {
    //     path "streamed";
    //     query { cursor; }
    //     body file;
    //     paginate next_batch(cursor);
    // }

    impl ::concord::page::Slot<shapes::Streamed> for shapes::__StreamedFile {
        type Value = ::concord::Body;
        fn get<'a>(&self, __call: &'a shapes::Streamed) -> ::core::option::Option<&'a Self::Value> {
            ::core::option::Option::Some(&__call.file)
        }
        fn set(&self, mut __call: shapes::Streamed, __value: Self::Value) -> shapes::Streamed {
            __call.file = __value;
            __call
        }
    }

    impl ::concord::page::Slot<shapes::Streamed> for shapes::__StreamedCursor {
        type Value = String;
        fn get<'a>(&self, __call: &'a shapes::Streamed) -> ::core::option::Option<&'a Self::Value> {
            __call.cursor.as_ref()
        }
        fn set(&self, mut __call: shapes::Streamed, __value: Self::Value) -> shapes::Streamed {
            __call.cursor = ::core::option::Option::Some(__value);
            __call
        }
    }

    /// The options of this endpoint.
    impl shapes::Streamed {
        /// The cursor of the page.
        ///
        /// Accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it.
        pub fn cursor(mut self, cursor: impl ::concord::arg::Arg<::core::option::Option<String>>) -> Self {
            self.cursor = cursor.into_input();
            self
        }
    }

    impl shapes::Streamed {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::POST);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("streamed")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let file = &self.file;
                let cursor = &self.cursor;
                __draft.query(
                    "cursor",
                    &[
                        ::concord::__private::Piece::Val(
                            "cursor",
                            &(&::concord::__private::ValueRef(cursor)).adapt(),
                        ),
                    ],
                );
            }
            #[allow(unused_variables)]
            {
                let client = __Client;
                let file = &self.file;
                let cursor = &self.cursor;
                #[allow(unused_braces, clippy::redundant_closure_call)]
                let __body = (|| ::std::result::Result::Ok({ file }))();
                let __body = match __body {
                    ::std::result::Result::Ok(__value) => __value,
                    ::std::result::Result::Err(__error) => {
                        return ::std::result::Result::Err(::concord::__private::build_error(
                            Self::__ENDPOINT,
                            __error,
                        ))
                    }
                };
                __draft.body(__body);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.streamed";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Batch>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Batch, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Batch, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Streamed,
        Batch,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Batch`]>",
        paged, unconditional,
        pages {
            /// Iterate over [`Batch`] pages.
            ///
            /// `try_next().await` gives [`shapes::Result`]<[`Option`]<[`Batch`]>>.
            /// Each page is one call. Nothing is asked until iteration starts.
            pub fn pages(self) -> ::concord::Pages<Batch, ShapesProblem> {
                // The owner in reach, then each input as its place; the expression is
                // copied as written.
                ::concord::__private::pages(self, {
                    #[allow(unused_variables)]
                    let client = __Client;
                    #[allow(unused_variables)]
                    let (file, cursor) = (
                        shapes::__StreamedFile,
                        shapes::__StreamedCursor,
                    );
                    next_batch(cursor)
                })
            }

        }
    );

    impl ::std::fmt::Debug for shapes::Streamed {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Streamed")
                .field("endpoint", &"shapes.streamed")
                .finish_non_exhaustive()
        }
    }

    // /// Pages of items asked with a body that is a new stream for each send.
    // POST renewed(
    //     /// The cursor of the page.
    //     cursor?: String,
    // ) -> Batch {
    //     path "renewed";
    //     query { cursor; }
    //     body stream();
    //     paginate next_batch(cursor);
    // }

    impl ::concord::page::Slot<shapes::Renewed> for shapes::__RenewedCursor {
        type Value = String;
        fn get<'a>(&self, __call: &'a shapes::Renewed) -> ::core::option::Option<&'a Self::Value> {
            __call.cursor.as_ref()
        }
        fn set(&self, mut __call: shapes::Renewed, __value: Self::Value) -> shapes::Renewed {
            __call.cursor = ::core::option::Option::Some(__value);
            __call
        }
    }

    /// The options of this endpoint.
    impl shapes::Renewed {
        /// The cursor of the page.
        ///
        /// Accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it.
        pub fn cursor(mut self, cursor: impl ::concord::arg::Arg<::core::option::Option<String>>) -> Self {
            self.cursor = cursor.into_input();
            self
        }
    }

    impl shapes::Renewed {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::POST);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("renewed")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let cursor = &self.cursor;
                __draft.query(
                    "cursor",
                    &[
                        ::concord::__private::Piece::Val(
                            "cursor",
                            &(&::concord::__private::ValueRef(cursor)).adapt(),
                        ),
                    ],
                );
            }
            #[allow(unused_variables)]
            {
                let client = __Client;
                let cursor = &self.cursor;
                #[allow(unused_braces, clippy::redundant_closure_call)]
                let __body = (|| ::std::result::Result::Ok({ stream() }))();
                let __body = match __body {
                    ::std::result::Result::Ok(__value) => __value,
                    ::std::result::Result::Err(__error) => {
                        return ::std::result::Result::Err(::concord::__private::build_error(
                            Self::__ENDPOINT,
                            __error,
                        ))
                    }
                };
                __draft.body(__body);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.renewed";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Batch>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Batch, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Batch, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Renewed,
        Batch,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Batch`]>",
        paged, unconditional,
        pages {
            /// Iterate over [`Batch`] pages.
            ///
            /// `try_next().await` gives [`shapes::Result`]<[`Option`]<[`Batch`]>>.
            /// Each page is one call. Nothing is asked until iteration starts.
            pub fn pages(self) -> ::concord::Pages<Batch, ShapesProblem> {
                // The owner in reach, then each input as its place; the expression is
                // copied as written.
                ::concord::__private::pages(self, {
                    #[allow(unused_variables)]
                    let client = __Client;
                    #[allow(unused_variables)]
                    let cursor = shapes::__RenewedCursor;
                    next_batch(cursor)
                })
            }

        }
    );

    impl ::std::fmt::Debug for shapes::Renewed {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Renewed")
                .field("endpoint", &"shapes.renewed")
                .finish_non_exhaustive()
        }
    }

    // /// Pages of items on a host whose first label is a key.
    // GET keyed_host(
    //     /// A key written as the first label of the host.
    //     host_key: Secret,
    //     /// The cursor of the page.
    //     cursor?: String,
    // ) -> Batch {
    //     base "https://{host_key}.shapes.example.com";
    //     path "keyed";
    //     query { cursor; }
    //     paginate next_batch(cursor);
    // }

    impl ::concord::page::Slot<shapes::KeyedHost> for shapes::__KeyedHostHostKey {
        type Value = Secret;
        fn get<'a>(&self, __call: &'a shapes::KeyedHost) -> ::core::option::Option<&'a Self::Value> {
            ::core::option::Option::Some(&__call.host_key)
        }
        fn set(&self, mut __call: shapes::KeyedHost, __value: Self::Value) -> shapes::KeyedHost {
            __call.host_key = __value;
            __call
        }
    }

    impl ::concord::page::Slot<shapes::KeyedHost> for shapes::__KeyedHostCursor {
        type Value = String;
        fn get<'a>(&self, __call: &'a shapes::KeyedHost) -> ::core::option::Option<&'a Self::Value> {
            __call.cursor.as_ref()
        }
        fn set(&self, mut __call: shapes::KeyedHost, __value: Self::Value) -> shapes::KeyedHost {
            __call.cursor = ::core::option::Option::Some(__value);
            __call
        }
    }

    /// The options of this endpoint.
    impl shapes::KeyedHost {
        /// The cursor of the page.
        ///
        /// Accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it.
        pub fn cursor(mut self, cursor: impl ::concord::arg::Arg<::core::option::Option<String>>) -> Self {
            self.cursor = cursor.into_input();
            self
        }
    }

    impl shapes::KeyedHost {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let host_key = &self.host_key;
                let cursor = &self.cursor;
                __draft.base(&[
                    ::concord::__private::Piece::Lit("https://"),
                    ::concord::__private::Piece::Val(
                        "host_key",
                        &(&::concord::__private::ValueRef(host_key)).adapt(),
                    ),
                    ::concord::__private::Piece::Lit(".shapes.example.com"),
                ]);
            }
            __draft.segment(&[::concord::__private::Piece::Lit("keyed")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let host_key = &self.host_key;
                let cursor = &self.cursor;
                __draft.query(
                    "cursor",
                    &[
                        ::concord::__private::Piece::Val(
                            "cursor",
                            &(&::concord::__private::ValueRef(cursor)).adapt(),
                        ),
                    ],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.keyed_host";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Batch>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Batch, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Batch, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::KeyedHost,
        Batch,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Batch`]>",
        paged, conditional,
        pages {
            /// Iterate over [`Batch`] pages.
            ///
            /// `try_next().await` gives [`shapes::Result`]<[`Option`]<[`Batch`]>>.
            /// Each page is one call. Nothing is asked until iteration starts.
            pub fn pages(self) -> ::concord::Pages<Batch, ShapesProblem> {
                // The owner in reach, then each input as its place; the expression is
                // copied as written.
                ::concord::__private::pages(self, {
                    #[allow(unused_variables)]
                    let client = __Client;
                    #[allow(unused_variables)]
                    let (host_key, cursor) = (
                        shapes::__KeyedHostHostKey,
                        shapes::__KeyedHostCursor,
                    );
                    next_batch(cursor)
                })
            }

        }
    );

    impl ::std::fmt::Debug for shapes::KeyedHost {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("KeyedHost")
                .field("endpoint", &"shapes.keyed_host")
                .finish_non_exhaustive()
        }
    }

    // /// Pages of items whose pager moves the tenant, which stands in the host.
    // GET roaming(
    //     /// The tenant: one label of the host.
    //     tenant: String,
    //     /// The cursor of the page.
    //     cursor?: String,
    // ) -> Batch {
    //     base "https://{tenant}.shapes.example.com";
    //     path "roaming";
    //     query { cursor; }
    //     paginate move |call, page| page.value().next.clone().map(|next| tenant.set(call, next));
    // }

    impl ::concord::page::Slot<shapes::Roaming> for shapes::__RoamingTenant {
        type Value = String;
        fn get<'a>(&self, __call: &'a shapes::Roaming) -> ::core::option::Option<&'a Self::Value> {
            ::core::option::Option::Some(&__call.tenant)
        }
        fn set(&self, mut __call: shapes::Roaming, __value: Self::Value) -> shapes::Roaming {
            __call.tenant = __value;
            __call
        }
    }

    impl ::concord::page::Slot<shapes::Roaming> for shapes::__RoamingCursor {
        type Value = String;
        fn get<'a>(&self, __call: &'a shapes::Roaming) -> ::core::option::Option<&'a Self::Value> {
            __call.cursor.as_ref()
        }
        fn set(&self, mut __call: shapes::Roaming, __value: Self::Value) -> shapes::Roaming {
            __call.cursor = ::core::option::Option::Some(__value);
            __call
        }
    }

    /// The options of this endpoint.
    impl shapes::Roaming {
        /// The cursor of the page.
        ///
        /// Accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it.
        pub fn cursor(mut self, cursor: impl ::concord::arg::Arg<::core::option::Option<String>>) -> Self {
            self.cursor = cursor.into_input();
            self
        }
    }

    impl shapes::Roaming {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let tenant = &self.tenant;
                let cursor = &self.cursor;
                __draft.base(&[
                    ::concord::__private::Piece::Lit("https://"),
                    ::concord::__private::Piece::Val(
                        "tenant",
                        &(&::concord::__private::ValueRef(tenant)).adapt(),
                    ),
                    ::concord::__private::Piece::Lit(".shapes.example.com"),
                ]);
            }
            __draft.segment(&[::concord::__private::Piece::Lit("roaming")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let tenant = &self.tenant;
                let cursor = &self.cursor;
                __draft.query(
                    "cursor",
                    &[
                        ::concord::__private::Piece::Val(
                            "cursor",
                            &(&::concord::__private::ValueRef(cursor)).adapt(),
                        ),
                    ],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.roaming";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Batch>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Batch, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Batch, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Roaming,
        Batch,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Batch`]>",
        paged, conditional,
        pages {
            /// Iterate over [`Batch`] pages.
            ///
            /// `try_next().await` gives [`shapes::Result`]<[`Option`]<[`Batch`]>>.
            /// Each page is one call. Nothing is asked until iteration starts.
            pub fn pages(self) -> ::concord::Pages<Batch, ShapesProblem> {
                // The owner in reach, then each input as its place; the expression is
                // copied as written.
                ::concord::__private::pages(self, {
                    #[allow(unused_variables)]
                    let client = __Client;
                    #[allow(unused_variables)]
                    let (tenant, cursor) = (
                        shapes::__RoamingTenant,
                        shapes::__RoamingCursor,
                    );
                    move |call, page| page.value().next.clone().map(|next| tenant.set(call, next))
                })
            }

        }
    );

    impl ::std::fmt::Debug for shapes::Roaming {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Roaming")
                .field("endpoint", &"shapes.roaming")
                .finish_non_exhaustive()
        }
    }

    // /// Pages of items that start at an offset.
    // GET offset(
    //     /// The index of the first item.
    //     start?: u32,
    // ) -> Vec<Item> {
    //     path "offset";
    //     query { start; }
    //     paginate by_offset(start);
    // }

    impl ::concord::page::Slot<shapes::Offset> for shapes::__OffsetStart {
        type Value = u32;
        fn get<'a>(&self, __call: &'a shapes::Offset) -> ::core::option::Option<&'a Self::Value> {
            __call.start.as_ref()
        }
        fn set(&self, mut __call: shapes::Offset, __value: Self::Value) -> shapes::Offset {
            __call.start = ::core::option::Option::Some(__value);
            __call
        }
    }

    /// The options of this endpoint.
    impl shapes::Offset {
        /// The index of the first item.
        ///
        /// Accepts [`u32`], &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it.
        pub fn start(mut self, start: impl ::concord::arg::Arg<::core::option::Option<u32>>) -> Self {
            self.start = start.into_input();
            self
        }
    }

    impl shapes::Offset {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("offset")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let start = &self.start;
                __draft.query(
                    "start",
                    &[
                        ::concord::__private::Piece::Val(
                            "start",
                            &(&::concord::__private::ValueRef(start)).adapt(),
                        ),
                    ],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.offset";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Vec<Item>>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Vec<Item>, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Vec<Item>, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Offset,
        Vec<Item>,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Vec`]<[`Item`]>>",
        paged, conditional,
        pages {
            /// Iterate over [`Vec`]<[`Item`]> pages.
            ///
            /// `try_next().await` gives [`shapes::Result`]<[`Option`]<[`Vec`]<[`Item`]>>>.
            /// Each page is one call. Nothing is asked until iteration starts.
            pub fn pages(self) -> ::concord::Pages<Vec<Item>, ShapesProblem> {
                // The owner in reach, then each input as its place; the expression is
                // copied as written.
                ::concord::__private::pages(self, {
                    #[allow(unused_variables)]
                    let client = __Client;
                    #[allow(unused_variables)]
                    let start = shapes::__OffsetStart;
                    by_offset(start)
                })
            }

        }
    );

    impl ::std::fmt::Debug for shapes::Offset {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Offset")
                .field("endpoint", &"shapes.offset")
                .finish_non_exhaustive()
        }
    }

    // /// Pages of items asked by their number.
    // GET numbered(
    //     /// The number of the page; the first is 1.
    //     page?: u32,
    // ) -> Vec<Item> {
    //     path "numbered";
    //     query { page; }
    //     paginate by_page(page, 1);
    // }

    impl ::concord::page::Slot<shapes::Numbered> for shapes::__NumberedPage {
        type Value = u32;
        fn get<'a>(&self, __call: &'a shapes::Numbered) -> ::core::option::Option<&'a Self::Value> {
            __call.page.as_ref()
        }
        fn set(&self, mut __call: shapes::Numbered, __value: Self::Value) -> shapes::Numbered {
            __call.page = ::core::option::Option::Some(__value);
            __call
        }
    }

    /// The options of this endpoint.
    impl shapes::Numbered {
        /// The number of the page; the first is 1.
        ///
        /// Accepts [`u32`], &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it.
        pub fn page(mut self, page: impl ::concord::arg::Arg<::core::option::Option<u32>>) -> Self {
            self.page = page.into_input();
            self
        }
    }

    impl shapes::Numbered {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("numbered")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let page = &self.page;
                __draft.query(
                    "page",
                    &[
                        ::concord::__private::Piece::Val(
                            "page",
                            &(&::concord::__private::ValueRef(page)).adapt(),
                        ),
                    ],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.numbered";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Vec<Item>>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Vec<Item>, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Vec<Item>, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Numbered,
        Vec<Item>,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Vec`]<[`Item`]>>",
        paged, conditional,
        pages {
            /// Iterate over [`Vec`]<[`Item`]> pages.
            ///
            /// `try_next().await` gives [`shapes::Result`]<[`Option`]<[`Vec`]<[`Item`]>>>.
            /// Each page is one call. Nothing is asked until iteration starts.
            pub fn pages(self) -> ::concord::Pages<Vec<Item>, ShapesProblem> {
                // The owner in reach, then each input as its place; the expression is
                // copied as written.
                ::concord::__private::pages(self, {
                    #[allow(unused_variables)]
                    let client = __Client;
                    #[allow(unused_variables)]
                    let page = shapes::__NumberedPage;
                    by_page(page, 1)
                })
            }

        }
    );

    impl ::std::fmt::Debug for shapes::Numbered {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Numbered")
                .field("endpoint", &"shapes.numbered")
                .finish_non_exhaustive()
        }
    }

    // /// Pages of items whose page gives the address of the next one.
    // GET addressed() -> Batch {
    //     path "addressed";
    //     paginate by_next_url(Batch::next_url);
    // }

    impl shapes::Addressed {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("addressed")]);
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.addressed";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Batch>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Batch, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Batch, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Addressed,
        Batch,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Batch`]>",
        paged, conditional,
        pages {
            /// Iterate over [`Batch`] pages.
            ///
            /// `try_next().await` gives [`shapes::Result`]<[`Option`]<[`Batch`]>>.
            /// Each page is one call. Nothing is asked until iteration starts.
            pub fn pages(self) -> ::concord::Pages<Batch, ShapesProblem> {
                // The owner in reach, then each input as its place; the expression is
                // copied as written.
                ::concord::__private::pages(self, {
                    #[allow(unused_variables)]
                    let client = __Client;
                    by_next_url(Batch::next_url)
                })
            }

        }
    );

    impl ::std::fmt::Debug for shapes::Addressed {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Addressed")
                .field("endpoint", &"shapes.addressed")
                .finish_non_exhaustive()
        }
    }

    // /// Remove an item. An accepted removal is not done; a missing item is removed.
    // DELETE remove(
    //     /// The identifier of the item.
    //     id: u64,
    // ) {
    //     path "items/{id}";
    //     status {
    //         202 => Err(concord::mismatch("an accepted removal is not done"))?,
    //         404 => (),
    //     }
    // }
    impl shapes::Remove {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::DELETE);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("items")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.remove";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<()>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            match __status {
                ::concord::http::StatusCode::ACCEPTED => {
                    // The arm of the endpoint: the owner in reach, then its inputs.
                    let client = __Client;
                    let id = &self.id;
                    // An arm under no arrow gives `()`: the author's expression is copied as written.
                    #[allow(unused_braces, clippy::unit_arg, clippy::unused_unit)]
                    let __arm = || -> ::std::result::Result<(), ::concord::Fail<ShapesProblem>> {
                        ::std::result::Result::Ok({ Err(concord::mismatch("an accepted removal is not done"))? })
                    };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                ::concord::http::StatusCode::NOT_FOUND => {
                    // The arm of the endpoint: the owner in reach, then its inputs.
                    let client = __Client;
                    let id = &self.id;
                    // An arm under no arrow gives `()`: the author's expression is copied as written.
                    #[allow(unused_braces, clippy::unit_arg, clippy::unused_unit)]
                    let __arm = || -> ::std::result::Result<(), ::concord::Fail<ShapesProblem>> {
                        ::std::result::Result::Ok({ () })
                    };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                __status if __status.is_success() => (),
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, ()).with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<(), ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Remove,
        (),
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>",
        awaitable,
        unconditional
    );

    impl ::std::fmt::Debug for shapes::Remove {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Remove")
                .field("endpoint", &"shapes.remove")
                .finish_non_exhaustive()
        }
    }

    // /// An item, or the conflict that holds it. An accepted change is a conflict too.
    // GET settled(
    //     /// The identifier of the item.
    //     id: u64,
    // ) -> Result<Item, Conflict> {
    //     path "settled/{id}";
    //     status { 202 => Err(Conflict { message: "pending".to_owned() }) }
    // }
    impl shapes::Settled {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("settled")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.settled";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Result<Item, Conflict>>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                ::concord::http::StatusCode::ACCEPTED => {
                    // The arm of the endpoint: the owner in reach, then its inputs.
                    let client = __Client;
                    let id = &self.id;
                    #[allow(unused_braces)]
                    let __arm = || -> ::std::result::Result<Result<Item, Conflict>, ::concord::Fail<ShapesProblem>> {
                        ::std::result::Result::Ok({ Err(Conflict {
                            message: "pending".to_owned(),
                        }) })
                    };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                __status if __status.is_success() => ::std::result::Result::Ok(
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Item, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?,
                ),
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Result<Item, Conflict>, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Settled,
        Result<Item, Conflict>,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Result`]<[`Item`], [`Conflict`]>>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::Settled {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Settled")
                .field("endpoint", &"shapes.settled")
                .finish_non_exhaustive()
        }
    }

    // /// A part of a file, as it comes, or none when there is nothing to send.
    // GET part(
    //     /// The identifier of the part.
    //     id: u64,
    // ) -> Option<concord::ByteStream> {
    //     path "parts/{id}";
    //     read Stream;
    //     status { 204 => None, 404 => None }
    // }
    impl shapes::Part {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("parts")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        const __ENDPOINT: &'static str = "shapes.part";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<Option<concord::ByteStream>>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                ::concord::http::StatusCode::NO_CONTENT => {
                    // The arm of the endpoint: the owner in reach, then its inputs.
                    let client = __Client;
                    let id = &self.id;
                    #[allow(unused_braces)]
                    let __arm = || -> ::std::result::Result<Option<concord::ByteStream>, ::concord::Fail<ShapesProblem>> {
                        ::std::result::Result::Ok({ None })
                    };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                ::concord::http::StatusCode::NOT_FOUND => {
                    // The arm of the endpoint: the owner in reach, then its inputs.
                    let client = __Client;
                    let id = &self.id;
                    #[allow(unused_braces)]
                    let __arm = || -> ::std::result::Result<Option<concord::ByteStream>, ::concord::Fail<ShapesProblem>> {
                        ::std::result::Result::Ok({ None })
                    };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                // `read Stream`: the try ends at the head, and the body is handed over unread.
                __status if __status.is_success() => ::core::option::Option::Some(
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            let id = &self.id;
                            (|| ::concord::__private::reader::<concord::ByteStream, _>(Stream))()
                        },
                        __response,
                        &__info,
                    )
                    .await?,
                ),
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<Option<concord::ByteStream>, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::Part,
        Option<concord::ByteStream>,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`Option`]<[`concord::ByteStream`]>>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::Part {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Part")
                .field("endpoint", &"shapes.part")
                .finish_non_exhaustive()
        }
    }

    // /// A file, as it comes, once the seal of its answer is checked.
    // GET file(
    //     /// The identifier of the file.
    //     id: u64,
    // ) -> concord::ByteStream {
    //     path "{id}";
    //     read Stream;
    // }
    impl shapes::sealed::File {
        fn __build_draft(&self) -> shapes::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://shapes.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("sealed")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let sealed = __Sealed;
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> shapes::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(
                    &self.__scope.__client.__core,
                    &self.__options,
                    Self::__ENDPOINT,
                )
            } else {
                __draft.finish(
                    &self.__scope.__client.__core,
                    &self.__options,
                    Self::__ENDPOINT,
                )
            }
        }

        const __ENDPOINT: &'static str = "shapes.sealed.file";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> shapes::Result<::concord::Answer<concord::ByteStream>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                // The arm of the scope: it takes every 2xx. `read Stream`: `body` is the stream,
                // handed over unread; the arm runs on the head.
                __status if __status.is_success() => {
                    let __body = ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            let sealed = __Sealed;
                            let id = &self.id;
                            (|| ::concord::__private::reader::<_, _>(Stream))()
                        },
                        __response,
                        &__info,
                    )
                    .await?;
                    // The arm of the scope: the owners in reach; the scope has no input.
                    let client = __Client;
                    let sealed = __Sealed;
                    #[allow(unused_braces)]
                    let __arm = |body, headers: &::concord::Headers| -> ::std::result::Result<
                        concord::ByteStream,
                        ::concord::Fail<ShapesProblem>,
                    > {
                        ::std::result::Result::Ok({ unseal(body, headers)? })
                    };
                    ::concord::__private::arm(
                        __arm(__body, &__headers),
                        __status,
                        &__headers,
                        &__info,
                    )?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> shapes::Result<::concord::__private::Prepared<concord::ByteStream, ShapesProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__scope.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__scope.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        shapes::sealed::File,
        concord::ByteStream,
        ShapesProblem,
        shapes::Result,
        "[`shapes::Result`]<[`concord::ByteStream`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for shapes::sealed::File {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("File")
                .field("endpoint", &"shapes.sealed.file")
                .finish_non_exhaustive()
        }
    }
};
