# The documentation of Concord

*[Concord](../README.md) · docs*

For anyone who uses Concord, or wants to: find the page for what you want to do. The language itself, with an example for each statement, is also the documentation of `concord::api!` on docs.rs (after the first upload; until then, [its source](../concord-macros/src/reference.md)).

## By what you want

| I want to… | Read |
|---|---|
| try it | [Concord in ten minutes](guide/tour.md): describe, call and test, code first. |
| describe an API | [Describe an API](guide/describing-an-api.md): every statement, with its rules; first, [how Concord sees an API](guide/model.md). |
| call it and handle what comes back | [Call a client](guide/calling.md), then [read answers and errors, send bodies](guide/responses-and-errors.md). |
| send a key, a token, a signature | [Credentials and middlewares](guide/credentials.md). |
| try again after a failure | [Retries](guide/retries.md). |
| get a list that comes in pages | [Pages](guide/pages.md). |
| set a timeout | [Call a client](guide/calling.md#time): the two clocks. |
| write tests | [Testing a client](guide/testing.md): a fake, a script on virtual time, a local server. |
| fix "it does not compile" or "the call failed" | [Troubleshooting](guide/troubleshooting.md), then [the messages](reference/messages.md). |
| do something Concord does not say | [When Concord does not say what you need](guide/extending.md): every door, with code. |
| look up a rule, a message, a status case | [The rules](reference/rules.md), [the messages](reference/messages.md), [the cases](reference/cases.md), [the grammar](reference/grammar.md). |
| see a real API | [Real APIs](examples/README.md): GitHub, Riot, S3, Stripe, Kubernetes, Slack, Telegram, Elasticsearch. |
| know exactly how it works inside | [The engine](internals/engine.md), [the macro](internals/macro.md): how one call runs, how code is written. |
| know why it is this way | [Why Concord](design/why.md), [alternatives](design/alternatives.md), [scope and limits](design/scope.md), [prior art](design/prior-art.md). |
| know what is promised | [What Concord promises](guide/promises.md): what never happens silently, and where it stops. |

## Every page

**[guide/](guide)**: read in this order.

1. [Concord in ten minutes](guide/tour.md): the whole library, one idea at a time, code first.
2. [How Concord sees an API](guide/model.md): one request, a tree, placement and behaviour.
3. [Describe an API](guide/describing-an-api.md): signatures, blocks, parts, middlewares, and the small print.
4. [Call a client](guide/calling.md): arguments, options, settings, what comes back, time.
5. [Read answers and errors, send bodies](guide/responses-and-errors.md): readers, status arms, the API's own error, bodies.
6. [Credentials and middlewares](guide/credentials.md): secrets, where they go, a secret and its base, signatures.
7. [Retries](guide/retries.md): when a request goes out again, policies, rules of your own.
8. [Pages](guide/pages.md): `paginate`, the shipped pagers, a pager of your own.
9. [Testing a client](guide/testing.md): a fake, a script on virtual time, a local server.
10. [What Concord promises](guide/promises.md): the promises, the safe defaults, the log, the limits.
11. [When Concord does not say what you need](guide/extending.md): writers, readers, pagers, middlewares, transports, and the raw doors.
12. [Troubleshooting](guide/troubleshooting.md): the first stop when it does not compile, or a call failed.

**[reference/](reference)**: to look things up.

- [Concord on one page](reference/overview.md): everything a description may say and a caller gets.
- [The grammar](reference/grammar.md): the whole language, on one page.
- [The rules](reference/rules.md): what a Rust developer could not guess from one example.
- [The messages](reference/messages.md): every message of the macro and of a call, word for word.
- [The crates](reference/crates.md): the five crates and their public surface.
- [The contracts](reference/contracts.md): the exact behaviour that the tests hold, by number.
- [The cases](reference/cases.md): every case that the rules decide, each with its one outcome.

**[examples/](examples)**: real APIs, written in the language.

- [Real APIs](examples/README.md), and one page for each: [GitHub](examples/github.md), [Riot Games](examples/riot.md), [an object store like Amazon S3](examples/s3.md), [Stripe](examples/stripe.md), [Kubernetes](examples/kubernetes.md), [Slack](examples/slack.md), [Telegram](examples/telegram.md), [Elasticsearch](examples/elasticsearch.md).

**[internals/](internals)**: how Concord works inside, to know exactly or to change it.

- [The engine](internals/engine.md): how one call runs: values, tries, clocks, pages.
- [The macro](internals/macro.md): how a description becomes code, and what it costs.
- [Engineering rules](internals/engineering.md): the rules the code keeps, and the check that holds each one.

**[design/](design)**: why Concord is this way.

- [Why Concord](design/why.md): the gap between an HTTP API and the code that calls it.
- [Alternatives](design/alternatives.md): for each choice, the strongest other choice.
- [Scope and limits](design/scope.md): what is outside, the tradeoffs, what can come later.
- [Prior art](design/prior-art.md): what known libraries do.

What changed from Concord 4 is in [the changelog](../CHANGELOG.md#coming-from-concord-4). To change Concord itself, start with [CONTRIBUTING.md](../CONTRIBUTING.md).
