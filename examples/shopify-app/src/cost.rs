//! Query-cost accounting shared by both workflows.

use concord_example_shopify_sdk::{QueryCost, ThrottleStatus};

/// What the GraphQL responses of one run reported in `extensions.cost`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CostLedger {
    /// Responses that carried a cost report.
    pub responses: u64,
    /// Decoded responses without one.
    pub without_cost: u64,
    /// Sum of `requestedQueryCost`.
    pub requested: f64,
    /// Sum of `actualQueryCost` where it was reported.
    pub actual: f64,
    /// Responses whose `actualQueryCost` was `null` (the query did not run).
    pub not_executed: u64,
    /// The bucket after the last response that reported one.
    pub last: Option<ThrottleStatus>,
    /// The lowest `currentlyAvailable` seen.
    pub lowest_available: Option<f64>,
}

impl CostLedger {
    /// Records one decoded response's cost report (or its absence).
    pub fn record(&mut self, cost: Option<&QueryCost>) {
        let Some(cost) = cost else {
            self.without_cost += 1;
            return;
        };
        self.responses += 1;
        self.requested += cost.requested_query_cost;
        match cost.actual_query_cost {
            Some(actual) => self.actual += actual,
            None => self.not_executed += 1,
        }
        let available = cost.throttle_status.currently_available;
        self.lowest_available = Some(
            self.lowest_available
                .map_or(available, |l| l.min(available)),
        );
        self.last = Some(cost.throttle_status);
    }
}
