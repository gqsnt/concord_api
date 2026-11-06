# Prediction probe (docs only): parameter groups and response metadata

You are a fresh reader of the Concord Rust library's public documentation. Your working
directory is a checkout, but for **Part 1** read only `docs/AUTHOR_GUIDE.md` sections 1, 2,
2.1, 4 and 5.11–5.13, `docs/CHEATSHEET.md` and `README.md`. Do **not** open `crates/`, `examples/`,
`checks/` or tests before you have written Part 1. You cannot modify files; your final message is
your whole answer. No private reasoning transcript is wanted — just predictions and evidence.

## Part 1 — predict (before reading any implementation)

Given this declaration:

```rust
concord::api! {
    pub client Desk(#[sensitive] header("authorization") token: String) {
        base "https://desk.example/api";
        format Json;

        params Paging(query("per_page") per_page?: u32 = 25, #[sensitive] query("cursor") cursor?: String);
        params Sort(query("sort") sort?: String);

        scope tickets {
            path "tickets";
            GET list(query("status") status?: String, ..Paging, ..Sort) -> TicketPage { path ""; }
            GET get(id: u64) -> Ticket { path "{id}"; }
            GET comments(id: u64, ..Paging) -> CommentPage { path "{id}/comments"; }
        }
    }
}
```

Answer each with a short prediction and your confidence (high/medium/low):

1. The exact URL of `desk.tickets().list().set_sort("-updated").set_status("open").prepare()?`.
2. The exact URL of `desk.tickets().list().maybe_per_page(None).prepare()?`.
3. Does `desk.tickets().get(7).set_cursor("x")` compile? Why?
4. What can `fn page<E: Paging>(e: E) -> E` call on `e`? Name the methods.
5. Where does `Debug` of a prepared `comments(1).set_cursor("abc")` show `abc`?
6. A send policy calls `received.decode_with_meta().await` and the 200 body is malformed JSON.
   What does the policy get, and can it still learn the status and a `x-request-id` header? How?
7. `received.document::<Json, Problem>(4096)` on a 503 whose `Content-Type` is `text/html` and
   whose body never ends: what returns, and when (before or after reading the body)?
8. Same call on a 422 JSON body of 10 KiB: what is the outcome's error kind, and is the status
   still available?
9. Is the response head cloned by `decode_with_meta` for a finite output? By `Error::meta()`?
10. Name one thing you think a parameter group must not allow, and what error you expect.

## Part 2 — check

Now you may read the implementation (`crates/concord-macros/src/{syntax,model,emit}.rs`,
`crates/concord-core/src/{error,response,prepared}.rs`) and the tests
(`checks/core-concepts/tests/`, `crates/concord/tests/ui/param_group_*`). For each prediction say
whether the source confirms it, and quote the symbol that decides it. Then list: documentation
statements that misled you or were missing; generated names or diagnostics that surprised you;
and anything that looks wrong or ambiguous in the design (with the smallest counterexample).
Do not run builds. Label clearly which statements are predictions and which are verified from
source.
