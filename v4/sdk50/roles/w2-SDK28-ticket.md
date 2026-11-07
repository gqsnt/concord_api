Agents need to change a ticket's status and add a comment without overwriting someone else's
concurrent edit. Zendesk's Tickets API documents updating a ticket (`PUT /api/v2/tickets/{id}`)
and "protecting against ticket update collisions" with `safe_update: true` plus the ticket's
`updated_stamp`, answered with `409 Conflict` when the ticket changed meanwhile (read the Tickets
reference sections on updating and on update collisions; cite them with the date). Add an update
operation that takes the stamp from a previously read ticket, reports a collision as its own
error (so the caller can re-read and decide), and treats a lost response like the SDK's other
uncertain writes (never resent automatically). Keep listing, show and create unchanged.
