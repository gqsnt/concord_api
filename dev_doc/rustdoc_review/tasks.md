# Reviewer tasks and rubric

The reviewer should be familiar with Rust libraries but must not be the primary
v2.1 implementer.

1. From generated Rustdoc alone, identify the response representation and its
   provenance for a Riot endpoint that inherits the client default.
2. Find the nested response override and state which endpoints it affects.
3. Find the request/response explicit fence and explain what a new endpoint
   below it must declare.
4. Read a parent-representation semantic diff and enumerate caller API, media,
   acquisition, capability, and affected-leaf consequences.
5. Identify any first-screen ambiguity, missing link, misleading term, or
   navigation failure in the generated docs.

For tasks 1–4 record correctness, completion seconds, files/pages opened,
corrections, confidence (1–5), and any high-confidence error. Task 5 records
each concrete problem and the commit that fixes it. Gate 10 requires correct
answers for tasks 1–4 and resolution or explicit release-blocking disposition
for every task-5 problem.
