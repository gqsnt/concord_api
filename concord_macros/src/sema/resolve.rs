#[derive(Debug, Default)]
#[allow(dead_code)]
pub(crate) struct ResolveStageReport {
    pub normalized_inputs: usize,
    pub resolved_endpoints: usize,
    pub facade_targets: usize,
    pub docs_targets: usize,
}

impl ResolveStageReport {
    #[allow(dead_code)]
    pub(crate) fn from_counts(
        normalized_inputs: usize,
        resolved_endpoints: usize,
        facade_targets: usize,
        docs_targets: usize,
    ) -> Self {
        Self {
            normalized_inputs,
            resolved_endpoints,
            facade_targets,
            docs_targets,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_stage_report_records_boundary_counts() {
        let report = ResolveStageReport::from_counts(1, 2, 3, 4);
        assert_eq!(report.normalized_inputs, 1);
        assert_eq!(report.resolved_endpoints, 2);
        assert_eq!(report.facade_targets, 3);
        assert_eq!(report.docs_targets, 4);
    }
}
