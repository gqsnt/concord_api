use std::fmt;

/// A value-free failure from checked public-structure accounting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StructuralLimitError {
    ComponentLimitExceeded,
    PublicByteLimitExceeded,
    ArithmeticOverflow,
}

impl fmt::Display for StructuralLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ComponentLimitExceeded => {
                formatter.write_str("structural component limit exceeded")
            }
            Self::PublicByteLimitExceeded => {
                formatter.write_str("structural public-byte limit exceeded")
            }
            Self::ArithmeticOverflow => formatter.write_str("structural accounting overflow"),
        }
    }
}

/// Checked accounting for retained public components and UTF-8 bytes.
#[derive(Clone, Copy, Default)]
pub(crate) struct StructuralBudget {
    components: usize,
    public_bytes: usize,
}

impl StructuralBudget {
    pub(crate) const fn new() -> Self {
        Self {
            components: 0,
            public_bytes: 0,
        }
    }

    #[cfg(test)]
    const fn from_counts(components: usize, public_bytes: usize) -> Self {
        Self {
            components,
            public_bytes,
        }
    }

    #[cfg(test)]
    pub(crate) const fn components(self) -> usize {
        self.components
    }

    #[cfg(test)]
    pub(crate) const fn public_bytes(self) -> usize {
        self.public_bytes
    }

    pub(crate) fn try_add(
        &mut self,
        components: usize,
        public_bytes: usize,
        maximum_components: usize,
        maximum_public_bytes: usize,
    ) -> Result<(), StructuralLimitError> {
        let next_components = self
            .components
            .checked_add(components)
            .ok_or(StructuralLimitError::ArithmeticOverflow)?;
        let next_public_bytes = self
            .public_bytes
            .checked_add(public_bytes)
            .ok_or(StructuralLimitError::ArithmeticOverflow)?;
        if next_components > maximum_components {
            return Err(StructuralLimitError::ComponentLimitExceeded);
        }
        if next_public_bytes > maximum_public_bytes {
            return Err(StructuralLimitError::PublicByteLimitExceeded);
        }
        self.components = next_components;
        self.public_bytes = next_public_bytes;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn try_replace(
        &mut self,
        previous_components: usize,
        previous_public_bytes: usize,
        replacement_components: usize,
        replacement_public_bytes: usize,
        maximum_components: usize,
        maximum_public_bytes: usize,
    ) -> Result<(), StructuralLimitError> {
        let next_components = self
            .components
            .checked_sub(previous_components)
            .and_then(|count| count.checked_add(replacement_components))
            .ok_or(StructuralLimitError::ArithmeticOverflow)?;
        let next_public_bytes = self
            .public_bytes
            .checked_sub(previous_public_bytes)
            .and_then(|bytes| bytes.checked_add(replacement_public_bytes))
            .ok_or(StructuralLimitError::ArithmeticOverflow)?;
        if next_components > maximum_components {
            return Err(StructuralLimitError::ComponentLimitExceeded);
        }
        if next_public_bytes > maximum_public_bytes {
            return Err(StructuralLimitError::PublicByteLimitExceeded);
        }
        self.components = next_components;
        self.public_bytes = next_public_bytes;
        Ok(())
    }
}

/// A consuming error latch whose updates run only while its state is valid.
pub(crate) struct ErrorLatch<T, E> {
    state: Result<T, E>,
}

impl<T, E> ErrorLatch<T, E> {
    pub(crate) const fn from_value(value: T) -> Self {
        Self { state: Ok(value) }
    }

    pub(crate) const fn from_error(error: E) -> Self {
        Self { state: Err(error) }
    }

    pub(crate) fn then<F>(self, update: F) -> Self
    where
        F: FnOnce(T) -> Result<T, E>,
    {
        Self {
            state: self.state.and_then(update),
        }
    }

    pub(crate) fn into_result(self) -> Result<T, E> {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SENTINEL: &str = "formatted-first-excess-sentinel-4d8e";

    #[test]
    fn structural_budget_checks_addition_replacement_and_overflow_before_mutation() {
        let mut budget = StructuralBudget::new();
        budget.try_add(2, 5, 3, 8).expect("initial budget");
        budget
            .try_replace(0, 2, 0, 4, 3, 8)
            .expect("replacement at boundary");
        assert_eq!(budget.components(), 2);
        assert_eq!(budget.public_bytes(), 7);

        let before = budget;
        assert_eq!(
            budget.try_add(2, 0, 3, 8),
            Err(StructuralLimitError::ComponentLimitExceeded)
        );
        assert_eq!(budget.components(), before.components());
        assert_eq!(budget.public_bytes(), before.public_bytes());

        assert_eq!(
            budget.try_replace(0, 8, 0, 0, 3, 8),
            Err(StructuralLimitError::ArithmeticOverflow)
        );
        assert_eq!(budget.public_bytes(), before.public_bytes());

        let mut overflow = StructuralBudget::from_counts(usize::MAX, usize::MAX);
        assert_eq!(
            overflow.try_add(1, 0, usize::MAX, usize::MAX),
            Err(StructuralLimitError::ArithmeticOverflow)
        );
        assert_eq!(
            overflow.try_add(0, 1, usize::MAX, usize::MAX),
            Err(StructuralLimitError::ArithmeticOverflow)
        );
        let rendered = format!(
            "{} {:?}",
            StructuralLimitError::ArithmeticOverflow,
            StructuralLimitError::ArithmeticOverflow
        );
        assert!(!rendered.contains(SENTINEL));
    }
}
