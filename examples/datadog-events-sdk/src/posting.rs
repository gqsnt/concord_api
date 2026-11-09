use crate::api::DatadogApiOperation;

/// Whether the operation is the legacy v1 post: success requires HTTP 202 and a failure after
/// dispatch is uncertain. Narrower than `!Operation::is_read()`: another write need not answer
/// 202. No wildcard arm: an endpoint added to the declaration without a decision here does not
/// compile (E0004, naming its `DatadogApiOperation` variant).
pub(crate) fn posting(operation: DatadogApiOperation) -> bool {
    match operation {
        DatadogApiOperation::EventsList | DatadogApiOperation::EventsSearch => false,
        DatadogApiOperation::EventsPost => true,
    }
}
