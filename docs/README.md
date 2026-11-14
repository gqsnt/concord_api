# Concord documentation

*[Concord](../README.md) · docs*

These pages are for anyone who uses Concord or is considering it. Find the page for what you want to do. The language reference, with an example for each statement, is the rustdoc page of `concord::api!` on docs.rs; until Concord is published, read [its source](../concord-macros/src/reference.md).

## Find a page by task

| I want to… | Read |
|---|---|
| try it | [Concord in ten minutes](guide/tour.md): describe, call and test, code first. |
| describe an API | [Describe an API](guide/describing-an-api.md): every statement, with its rules; first, [How Concord sees an API](guide/model.md). |
| call it and handle the result | [Call a client](guide/calling.md), then [Read responses and errors, send bodies](guide/responses-and-errors.md). |
| send a key, a token or a signature | [Credentials and middleware](guide/credentials.md). |
| retry after a failure | [Retries](guide/retries.md). |
| fetch a paginated list | [Pages](guide/pages.md). |
| set a timeout | [Call a client](guide/calling.md#timeouts): the idle timeout and the call timeout. |
| decompress responses, use a SOCKS proxy, choose TLS | [Call a client](guide/calling.md#compression-proxies-and-tls): the features of `concord-api`. |
| write tests | [Testing a client](guide/testing.md): a fake transport, a scripted transport on virtual time, a local server. |
| fix "it does not compile" or "the call failed" | [Troubleshooting](guide/troubleshooting.md), then [The messages](reference/messages.md). |
| do something Concord does not cover | [When Concord does not cover what you need](guide/extending.md): every extension point and escape hatch, with code. |
| look up a rule, a message or a status case | [The rules](reference/rules.md), [The messages](reference/messages.md), [The cases](reference/cases.md), [The grammar](reference/grammar.md). |
| see a real API | [Real APIs](examples/README.md): GitHub, Riot, S3, Stripe, Kubernetes, Slack, Telegram, Elasticsearch. |
| know exactly how it works inside | [The engine](internals/engine.md), [The macro](internals/macro.md): how one call runs, and how the code is generated. |
| know why it is designed this way | [Why Concord](design/why.md), [Alternatives](design/alternatives.md), [Scope and limits](design/scope.md), [Prior art](design/prior-art.md). |
| know what Concord guarantees | [What Concord promises](guide/promises.md): what Concord guarantees, and where each guarantee stops. |

## All pages

**[guide/](guide)**: read these in order.

1. [Concord in ten minutes](guide/tour.md): the whole library, one idea at a time, code first.
2. [How Concord sees an API](guide/model.md): the request and the response, the tree of nodes, placement and behaviour.
3. [Describe an API](guide/describing-an-api.md): signatures, blocks, parts, middleware, and the details.
4. [Call a client](guide/calling.md): arguments, options, settings, return values and errors, timeouts.
5. [Read responses and errors, send bodies](guide/responses-and-errors.md): readers, status arms, the API's own error, request bodies.
6. [Credentials and middleware](guide/credentials.md): secrets, where they are placed, why a secret stays with its base, signatures.
7. [Retries](guide/retries.md): when a request is retried, policies, your own rules.
8. [Pages](guide/pages.md): `paginate`, the shipped pagers, your own pager.
9. [Testing a client](guide/testing.md): a fake transport, a scripted transport on virtual time, a local server.
10. [What Concord promises](guide/promises.md): the promises, the safe defaults, logging, the limits.
11. [When Concord does not cover what you need](guide/extending.md): extension points (body writers, readers, pagers, middleware, transports) and the raw escape hatches.
12. [Troubleshooting](guide/troubleshooting.md): the first stop when a description does not compile or a call fails.

**[reference/](reference)**: for looking things up.

- [Concord on one page](reference/overview.md): everything a description can say, and everything a caller gets.
- [The grammar](reference/grammar.md): the whole language on one page.
- [The rules](reference/rules.md): what a Rust developer could not guess from one example.
- [The messages](reference/messages.md): every message from the macro and from a call, word for word.
- [The crates](reference/crates.md): the five crates and their public API.
- [The contracts](reference/contracts.md): the exact behaviour that the tests enforce, by number.
- [The cases](reference/cases.md): every case that the rules decide, each with its one outcome.

**[examples/](examples)**: real APIs, described in the language.

- [Real APIs](examples/README.md), and one page for each: [GitHub](examples/github.md), [Riot Games](examples/riot.md), [an object store like Amazon S3](examples/s3.md), [Stripe](examples/stripe.md), [Kubernetes](examples/kubernetes.md), [Slack](examples/slack.md), [Telegram](examples/telegram.md), [Elasticsearch](examples/elasticsearch.md).

**[internals/](internals)**: how Concord works inside, for when you need the exact details or want to change it.

- [The engine](internals/engine.md): how one call runs: values, attempts, timeouts, pages.
- [The macro](internals/macro.md): how a description becomes code, and what it costs.
- [Engineering rules](internals/engineering.md): the rules the code follows, and the check that enforces each one.

**[design/](design)**: why Concord is this way.

- [Why Concord](design/why.md): the gap between an HTTP API and the code that calls it.
- [Alternatives](design/alternatives.md): for each design choice, the strongest alternative.
- [Scope and limits](design/scope.md): what is out of scope, the tradeoffs, what may come later.
- [Prior art](design/prior-art.md): how existing libraries solve the same problems.

See [the changelog](../CHANGELOG.md#coming-from-concord-4) for what changed since Concord 4. To change Concord itself, start with [CONTRIBUTING.md](../CONTRIBUTING.md).
