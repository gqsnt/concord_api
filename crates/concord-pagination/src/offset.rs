//! Checked continuation for offset-based pages.

/// An inconsistent offset page. No provider data is retained or displayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffsetError {
    /// The page limit is zero.
    ZeroLimit,
    /// The page contains more items than its limit.
    TooManyItems,
    /// The next offset cannot be represented as a `u64`.
    Overflow,
    /// The offset or page end exceeds the reported total count.
    BeyondCount,
    /// An empty page leaves items remaining in the reported total count.
    DidNotAdvance,
}

impl std::fmt::Display for OffsetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ZeroLimit => "the page limit is zero",
            Self::TooManyItems => "the page contains more items than the limit",
            Self::Overflow => "the next offset overflows",
            Self::BeyondCount => "the offset or page end exceeds the total count",
            Self::DidNotAdvance => "the page did not advance despite remaining items",
        })
    }
}

impl std::error::Error for OffsetError {}

/// Validate a page and return its next offset, or `None` for clean exhaustion.
///
/// `len` is the number of returned items; `limit` is the allowed page size.
/// With `Some(total)`, `offset` is zero-based (items already skipped): the page must
/// end at or before `total`, and an empty page before `total` is an error. A short
/// page can still have a continuation. Providers with one-based offsets must first
/// project them to zero-based offsets when using a total count.
///
/// With `None`, a short page ends traversal and a full page requires another request;
/// offsets may use either base. No unused next offset is computed for a short page.
/// All required offset addition is checked. Zero limits and overfull pages always fail.
///
/// The SDK still checks provider-specific facts, such as an echoed offset or a reported
/// page length, and projects the result into [`Page`](crate::Page).
///
/// ```
/// use concord_pagination::next_offset;
/// assert_eq!(next_offset(20, 10, 10, Some(35)), Ok(Some(30)));
/// assert_eq!(next_offset(30, 10, 5, Some(35)), Ok(None));
/// assert_eq!(next_offset(1, 10, 10, None), Ok(Some(11)));
/// ```
pub fn next_offset(
    offset: u64,
    limit: u64,
    len: u64,
    total: Option<u64>,
) -> Result<Option<u64>, OffsetError> {
    if limit == 0 {
        return Err(OffsetError::ZeroLimit);
    }
    if len > limit {
        return Err(OffsetError::TooManyItems);
    }
    if total.is_none() && len < limit {
        return Ok(None);
    }
    let end = offset.checked_add(len).ok_or(OffsetError::Overflow)?;
    if let Some(total) = total {
        if end > total {
            return Err(OffsetError::BeyondCount);
        }
        if end == total {
            return Ok(None);
        }
        if end == offset {
            return Err(OffsetError::DidNotAdvance);
        }
    }
    Ok(Some(end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow() {
        for total in [None, Some(u64::MAX)] {
            assert_eq!(
                next_offset(u64::MAX, 1, 1, total),
                Err(OffsetError::Overflow)
            );
        }
    }

    #[test]
    fn zero_limit() {
        for total in [None, Some(0), Some(1)] {
            assert_eq!(next_offset(0, 0, 0, total), Err(OffsetError::ZeroLimit));
        }
    }

    #[test]
    fn empty_page_with_remaining_count() {
        assert_eq!(
            next_offset(2, 3, 0, Some(5)),
            Err(OffsetError::DidNotAdvance)
        );
    }

    #[test]
    fn exact_end() {
        assert_eq!(next_offset(2, 3, 3, Some(5)), Ok(None));
        assert_eq!(next_offset(0, 3, 0, Some(0)), Ok(None));
        assert_eq!(next_offset(5, 3, 0, Some(5)), Ok(None));
        assert_eq!(next_offset(u64::MAX - 1, 1, 1, Some(u64::MAX)), Ok(None));
    }

    #[test]
    fn overfull_page() {
        for total in [None, Some(10)] {
            assert_eq!(next_offset(0, 2, 3, total), Err(OffsetError::TooManyItems));
        }
    }

    #[test]
    fn beyond_count() {
        assert_eq!(next_offset(6, 3, 0, Some(5)), Err(OffsetError::BeyondCount));
        assert_eq!(next_offset(4, 3, 2, Some(5)), Err(OffsetError::BeyondCount));
    }

    #[test]
    fn total_controls_short_page_continuation() {
        assert_eq!(next_offset(2, 3, 1, Some(5)), Ok(Some(3)));
        assert_eq!(next_offset(2, 3, 1, None), Ok(None));
    }

    #[test]
    fn unknown_total_uses_page_length_and_preserves_offset_base() {
        assert_eq!(next_offset(0, 2, 2, None), Ok(Some(2)));
        assert_eq!(next_offset(1, 2, 2, None), Ok(Some(3)));
        assert_eq!(next_offset(u64::MAX, 2, 1, None), Ok(None));
        assert_eq!(next_offset(u64::MAX, 2, 0, None), Ok(None));
    }
}
