# Contract Event Schemas

OrbitChain emits typed Soroban events using `#[contractevent]`; it no longer
uses the deprecated `Events::publish` API. The macro fixes each event's first
topic to its lower-snake-case struct name and encodes the remaining fields as a
named map. `#[topic]` fields are appended as dynamic, indexable topics.

The complete generated topic schema is in
[`events.generated.md`](events.generated.md). Its contents are pinned by
`campaign/src/test/events_test.rs`, which asserts the macro-generated event
topic and serialized payload.

## Indexing convention

- Filter by the first topic (for example, `donation_received`) to select an
  event class.
- Use subsequent topics for the declared entity keys such as `donor`,
  `creator`, `campaign_id`, or `milestone_index`.
- Read all non-topic fields from the named event-data map.

The legacy `orbitchain-core` contract follows the same convention for
`campaign_created`, `donation_received`, `withdrawal_requested`,
`withdrawal_approved`, and `transaction_submitted`.
