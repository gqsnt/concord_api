use std::fmt::{self, Display, Write};

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

    pub(crate) fn remaining_public_bytes(
        self,
        maximum: usize,
    ) -> Result<usize, StructuralLimitError> {
        maximum
            .checked_sub(self.public_bytes)
            .ok_or(StructuralLimitError::PublicByteLimitExceeded)
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

    pub(crate) fn try_replace_public_bytes(
        &mut self,
        previous_bytes: usize,
        replacement_bytes: usize,
        maximum_public_bytes: usize,
    ) -> Result<(), StructuralLimitError> {
        let next_public_bytes = self
            .public_bytes
            .checked_sub(previous_bytes)
            .and_then(|bytes| bytes.checked_add(replacement_bytes))
            .ok_or(StructuralLimitError::ArithmeticOverflow)?;
        if next_public_bytes > maximum_public_bytes {
            return Err(StructuralLimitError::PublicByteLimitExceeded);
        }
        self.public_bytes = next_public_bytes;
        Ok(())
    }
}

/// A structural formatting failure that never retains the formatted value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BoundedFormatError {
    LimitExceeded,
    FormattingFailed,
}

impl fmt::Display for BoundedFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitExceeded => formatter.write_str("formatted value limit exceeded"),
            Self::FormattingFailed => formatter.write_str("value formatting failed"),
        }
    }
}

struct CappedString {
    output: String,
    maximum_bytes: usize,
    exceeded: bool,
    failed: bool,
}

impl CappedString {
    fn new(maximum_bytes: usize) -> Self {
        Self {
            output: String::new(),
            maximum_bytes,
            exceeded: false,
            failed: false,
        }
    }
}

impl Write for CappedString {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if self.exceeded || self.failed {
            return Err(fmt::Error);
        }
        let Some(next_length) = self.output.len().checked_add(value.len()) else {
            self.exceeded = true;
            return Err(fmt::Error);
        };
        if next_length > self.maximum_bytes {
            self.exceeded = true;
            return Err(fmt::Error);
        }
        if self.output.try_reserve_exact(value.len()).is_err() {
            self.failed = true;
            return Err(fmt::Error);
        }
        self.output.push_str(value);
        Ok(())
    }
}

/// Formats into a UTF-8 byte-capped Concord-owned string.
///
/// User `Display` code remains responsible for bounding its own CPU work and
/// may panic or ignore formatting failures. This function bounds only the
/// output retained by Concord.
pub(crate) fn format_bounded<T>(
    value: &T,
    maximum_bytes: usize,
) -> Result<String, BoundedFormatError>
where
    T: Display + ?Sized,
{
    let mut output = CappedString::new(maximum_bytes);
    let formatting = write!(&mut output, "{value}");
    if output.exceeded {
        Err(BoundedFormatError::LimitExceeded)
    } else if output.failed || formatting.is_err() {
        Err(BoundedFormatError::FormattingFailed)
    } else {
        Ok(output.output)
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

    struct IgnoreFormattingFailure;

    impl Display for IgnoreFormattingFailure {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("1234")?;
            let _ = formatter.write_str(SENTINEL);
            Ok(())
        }
    }

    #[test]
    fn structural_budget_checks_addition_replacement_and_overflow_before_mutation() {
        let mut budget = StructuralBudget::new();
        budget.try_add(2, 5, 3, 8).expect("initial budget");
        budget
            .try_replace_public_bytes(2, 4, 8)
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
            budget.try_replace_public_bytes(8, 0, 8),
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

    #[test]
    fn capped_formatting_uses_utf8_bytes_and_rejects_ignored_first_excess() {
        let maximum = "x".repeat(crate::limits::MAX_FORMATTED_PUBLIC_VALUE_BYTES);
        assert_eq!(
            format_bounded(&maximum, crate::limits::MAX_FORMATTED_PUBLIC_VALUE_BYTES)
                .expect("canonical formatted-value boundary")
                .len(),
            crate::limits::MAX_FORMATTED_PUBLIC_VALUE_BYTES
        );
        assert_eq!(format_bounded("éé", 4).expect("exact UTF-8 boundary"), "éé");
        assert_eq!(
            format_bounded("ééx", 4),
            Err(BoundedFormatError::LimitExceeded)
        );

        let error = format_bounded(&IgnoreFormattingFailure, 4)
            .expect_err("ignored first excess must remain a failure");
        assert_eq!(error, BoundedFormatError::LimitExceeded);
        let rendered = format!("{error} {error:?}");
        assert!(!rendered.contains(SENTINEL));
    }

    #[test]
    fn capped_writer_withholds_the_complete_first_excess_chunk() {
        let mut output = CappedString::new(4);
        output.write_str("1234").expect("exact boundary");
        assert!(output.write_str(SENTINEL).is_err());
        assert_eq!(output.output, "1234");
        assert!(!output.output.contains(SENTINEL));
    }
}
