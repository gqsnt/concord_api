//! Byte-exact source edits, line/column mapping and unified diff rendering.
//!
//! The migration never re-prints a whole Rust file. It records replacements of
//! exact byte ranges that were located through real `syn` token spans, so
//! comments, formatting and every untouched token stay byte-for-byte equal.

use std::ops::Range;

/// A half-open byte range into one source file.
pub(crate) type Rng = Range<usize>;

#[derive(Clone, Debug)]
struct Edit {
    range: Rng,
    text: String,
    sequence: usize,
}

/// A set of non-overlapping edits over one source text.
///
/// Zero-length insertions at the same offset are applied in recording order.
/// A replacement may subsume previously recorded inner edits only through
/// [`EditSet::replace_subsuming`], which renders them first so nested
/// conversions compose without guessing.
#[derive(Clone, Debug, Default)]
pub(crate) struct EditSet {
    edits: Vec<Edit>,
    next_sequence: usize,
}

impl EditSet {
    pub(crate) fn len(&self) -> usize {
        self.edits.len()
    }

    /// Inserts text before the byte at `offset`.
    pub(crate) fn insert(&mut self, offset: usize, text: impl Into<String>) -> Result<(), String> {
        self.replace(offset..offset, text)
    }

    /// Replaces one exact range. Partial overlap with an existing edit is an
    /// internal error rather than a silent last-writer-wins merge.
    pub(crate) fn replace(&mut self, range: Rng, text: impl Into<String>) -> Result<(), String> {
        let text = text.into();
        if range.start > range.end {
            return Err(format!("invalid edit range {range:?}"));
        }
        if let Some(conflict) = self.edits.iter().find(|edit| overlaps(&edit.range, &range)) {
            return Err(format!(
                "conflicting source edits at bytes {:?} and {range:?}",
                conflict.range
            ));
        }
        self.push(range, text);
        Ok(())
    }

    /// Replaces `range`, discarding every edit fully inside it. Callers render
    /// the inner edits with [`EditSet::render`] first when they must be kept.
    pub(crate) fn replace_subsuming(
        &mut self,
        range: Rng,
        text: impl Into<String>,
    ) -> Result<(), String> {
        self.edits.retain(|edit| !contains(&range, &edit.range));
        self.replace(range, text)
    }

    /// Renders `range` of `source` with every edit fully inside it applied.
    pub(crate) fn render(&self, source: &str, range: Rng) -> String {
        let mut inner = self
            .edits
            .iter()
            .filter(|edit| contains(&range, &edit.range))
            .cloned()
            .collect::<Vec<_>>();
        sort(&mut inner);
        apply_sorted(source, range, &inner)
    }

    /// Applies every edit to the complete source.
    pub(crate) fn apply(&self, source: &str) -> String {
        let mut edits = self.edits.clone();
        sort(&mut edits);
        apply_sorted(source, 0..source.len(), &edits)
    }

    fn push(&mut self, range: Rng, text: String) {
        self.edits.push(Edit {
            range,
            text,
            sequence: self.next_sequence,
        });
        self.next_sequence += 1;
    }
}

fn overlaps(left: &Rng, right: &Rng) -> bool {
    if left.is_empty() && right.is_empty() {
        return false;
    }
    if left.is_empty() {
        return left.start > right.start && left.start < right.end;
    }
    if right.is_empty() {
        return right.start > left.start && right.start < left.end;
    }
    left.start < right.end && right.start < left.end
}

fn contains(outer: &Rng, inner: &Rng) -> bool {
    outer.start <= inner.start && inner.end <= outer.end && outer != inner
}

fn sort(edits: &mut [Edit]) {
    edits.sort_by(|left, right| {
        (left.range.start, !left.range.is_empty(), left.sequence).cmp(&(
            right.range.start,
            !right.range.is_empty(),
            right.sequence,
        ))
    });
}

fn apply_sorted(source: &str, range: Rng, edits: &[Edit]) -> String {
    let mut output = String::with_capacity(range.len() + 64);
    let mut cursor = range.start;
    for edit in edits {
        output.push_str(&source[cursor..edit.range.start]);
        output.push_str(&edit.text);
        cursor = edit.range.end;
    }
    output.push_str(&source[cursor..range.end]);
    output
}

/// Maps byte offsets to one-based lines and one-based character columns.
pub(crate) struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    pub(crate) fn new(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(index, _)| index + 1),
        );
        Self { starts }
    }

    pub(crate) fn line_col(&self, source: &str, offset: usize) -> (usize, usize) {
        let offset = offset.min(source.len());
        let line = match self.starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index - 1,
        };
        let start = self.starts[line];
        let column = source
            .get(start..offset)
            .map_or(offset - start, |text| text.chars().count());
        (line + 1, column + 1)
    }
}

/// Returns the horizontal whitespace that begins the line containing `offset`.
pub(crate) fn line_indent(source: &str, offset: usize) -> &str {
    let start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let rest = &source[start..];
    let width = rest
        .bytes()
        .take_while(|byte| *byte == b' ' || *byte == b'\t')
        .count();
    &rest[..width]
}

/// Extends a removal so that a construct occupying whole lines removes those
/// lines, and an inline construct also removes its trailing spaces.
pub(crate) fn removal_range(source: &str, range: Rng) -> Rng {
    let bytes = source.as_bytes();
    let mut start = range.start;
    while start > 0 && matches!(bytes[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    let line_start = start == 0 || bytes[start - 1] == b'\n';
    let mut end = range.end;
    while end < bytes.len() && matches!(bytes[end], b' ' | b'\t') {
        end += 1;
    }
    let line_end = end == bytes.len() || bytes[end] == b'\n' || bytes[end] == b'\r';
    if line_start && line_end {
        end = past_newline(bytes, end);
        // Do not leave two blank lines where the construct separated them.
        let previous_blank = start == 0 || {
            let before = &source[..start - 1];
            before
                .rfind('\n')
                .map_or(before, |index| &before[index + 1..])
                .trim()
                .is_empty()
        };
        let mut next = end;
        while next < bytes.len() && matches!(bytes[next], b' ' | b'\t') {
            next += 1;
        }
        let next_blank = next < bytes.len() && matches!(bytes[next], b'\n' | b'\r');
        if previous_blank && next_blank {
            end = past_newline(bytes, next);
        }
        return start..end;
    }
    range.start..end
}

fn past_newline(bytes: &[u8], mut end: usize) -> usize {
    if end < bytes.len() && bytes[end] == b'\r' {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'\n' {
        end += 1;
    }
    end
}

/// Renders a unified diff with three context lines. Returns an empty string
/// when the texts are equal.
pub(crate) fn unified_diff(path: &str, old: &str, new: &str) -> String {
    if old == new {
        return String::new();
    }
    let old_lines = split_lines(old);
    let new_lines = split_lines(new);
    let script = diff_lines(&old_lines, &new_lines);
    let mut output = format!("--- a/{path}\n+++ b/{path}\n");
    for hunk in hunks(&script, 3) {
        let (old_start, old_count, new_start, new_count) = hunk_header(&script, &hunk);
        output.push_str(&format!(
            "@@ -{} +{} @@\n",
            range_label(old_start, old_count),
            range_label(new_start, new_count)
        ));
        for operation in &script[hunk] {
            let (marker, line) = match operation {
                Operation::Equal(old_index, _) => (' ', old_lines[*old_index]),
                Operation::Delete(old_index) => ('-', old_lines[*old_index]),
                Operation::Insert(new_index) => ('+', new_lines[*new_index]),
            };
            output.push(marker);
            if let Some(stripped) = line.strip_suffix('\n') {
                output.push_str(stripped);
                output.push('\n');
            } else {
                output.push_str(line);
                output.push_str("\n\\ No newline at end of file\n");
            }
        }
    }
    output
}

fn range_label(start: usize, count: usize) -> String {
    if count == 1 {
        format!("{start}")
    } else {
        format!("{start},{count}")
    }
}

fn split_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Operation {
    Equal(usize, usize),
    Delete(usize),
    Insert(usize),
}

/// Bounded Myers line diff after trimming the common prefix and suffix. A
/// pathological edit distance falls back to one replacement block, which is
/// still a correct (if less minimal) patch.
fn diff_lines(old: &[&str], new: &[&str]) -> Vec<Operation> {
    let prefix = old
        .iter()
        .zip(new.iter())
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let old_middle = &old[prefix..old.len() - suffix];
    let new_middle = &new[prefix..new.len() - suffix];

    let mut script = (0..prefix)
        .map(|index| Operation::Equal(index, index))
        .collect::<Vec<_>>();
    match myers(old_middle, new_middle, 1_000) {
        Some(middle) => script.extend(middle.into_iter().map(|operation| match operation {
            Operation::Equal(left, right) => Operation::Equal(left + prefix, right + prefix),
            Operation::Delete(left) => Operation::Delete(left + prefix),
            Operation::Insert(right) => Operation::Insert(right + prefix),
        })),
        None => {
            script.extend((0..old_middle.len()).map(|index| Operation::Delete(index + prefix)));
            script.extend((0..new_middle.len()).map(|index| Operation::Insert(index + prefix)));
        }
    }
    let old_tail = old.len() - suffix;
    let new_tail = new.len() - suffix;
    script.extend((0..suffix).map(|index| Operation::Equal(old_tail + index, new_tail + index)));
    script
}

fn myers(old: &[&str], new: &[&str], max_distance: usize) -> Option<Vec<Operation>> {
    let n = old.len();
    let m = new.len();
    let limit = (n + m).min(max_distance);
    let offset = limit as isize + 1;
    let width = 2 * limit + 3;
    let mut v = vec![0_usize; width];
    let mut trace: Vec<Vec<usize>> = Vec::new();
    let mut found = None;
    for distance in 0..=limit as isize {
        trace.push(v.clone());
        let mut k = -distance;
        while k <= distance {
            let index = (k + offset) as usize;
            let mut x = if k == -distance || (k != distance && v[index - 1] < v[index + 1]) {
                v[index + 1]
            } else {
                v[index - 1] + 1
            };
            let mut y = (x as isize - k) as usize;
            while x < n && y < m && old[x] == new[y] {
                x += 1;
                y += 1;
            }
            v[index] = x;
            if x >= n && y >= m {
                found = Some(distance);
                break;
            }
            k += 2;
        }
        if found.is_some() {
            break;
        }
    }
    let distance = found?;
    let mut script = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (1..=distance).rev() {
        let v = &trace[d as usize];
        let k = x as isize - y as isize;
        let index = (k + offset) as usize;
        let previous_k = if k == -d || (k != d && v[index - 1] < v[index + 1]) {
            k + 1
        } else {
            k - 1
        };
        let previous_x = v[(previous_k + offset) as usize];
        let previous_y = (previous_x as isize - previous_k) as usize;
        while x > previous_x && y > previous_y {
            x -= 1;
            y -= 1;
            script.push(Operation::Equal(x, y));
        }
        if x == previous_x {
            y -= 1;
            script.push(Operation::Insert(y));
        } else {
            x -= 1;
            script.push(Operation::Delete(x));
        }
    }
    while x > 0 && y > 0 {
        x -= 1;
        y -= 1;
        script.push(Operation::Equal(x, y));
    }
    script.reverse();
    Some(script)
}

fn hunks(script: &[Operation], context: usize) -> Vec<Rng> {
    let changes = script
        .iter()
        .enumerate()
        .filter(|(_, operation)| !matches!(operation, Operation::Equal(..)))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let mut result: Vec<Rng> = Vec::new();
    for index in changes {
        let start = index.saturating_sub(context);
        let end = (index + context + 1).min(script.len());
        match result.last_mut() {
            Some(last) if start <= last.end => last.end = last.end.max(end),
            _ => result.push(start..end),
        }
    }
    result
}

fn hunk_header(script: &[Operation], hunk: &Rng) -> (usize, usize, usize, usize) {
    let mut old_count = 0;
    let mut new_count = 0;
    let mut old_start = None;
    let mut new_start = None;
    for operation in &script[hunk.clone()] {
        match operation {
            Operation::Equal(left, right) => {
                old_start.get_or_insert(*left);
                new_start.get_or_insert(*right);
                old_count += 1;
                new_count += 1;
            }
            Operation::Delete(left) => {
                old_start.get_or_insert(*left);
                old_count += 1;
            }
            Operation::Insert(right) => {
                new_start.get_or_insert(*right);
                new_count += 1;
            }
        }
    }
    // A pure insertion/deletion side reports the line before the hunk.
    let old_start = old_start.map_or_else(
        || preceding_position(script, hunk.start, true),
        |start| start + 1,
    );
    let new_start = new_start.map_or_else(
        || preceding_position(script, hunk.start, false),
        |start| start + 1,
    );
    (old_start, old_count, new_start, new_count)
}

fn preceding_position(script: &[Operation], before: usize, old_side: bool) -> usize {
    script[..before]
        .iter()
        .filter(|operation| match operation {
            Operation::Equal(..) => true,
            Operation::Delete(_) => old_side,
            Operation::Insert(_) => !old_side,
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_edits_render_and_subsume_without_overlap() {
        let source = "key [client.tenant];";
        let mut edits = EditSet::default();
        edits.insert(5, "\"client.tenant\" = ").expect("insert");
        let rendered = edits.render(source, 0..source.len());
        assert_eq!(rendered, "key [\"client.tenant\" = client.tenant];");
        assert!(edits.replace(3..8, "x").is_err());
        edits
            .replace_subsuming(0..source.len(), rendered.clone())
            .expect("subsume");
        assert_eq!(edits.apply(source), rendered);
    }

    #[test]
    fn whole_line_removal_includes_the_newline() {
        let source = "a\n    query { \"k\" = endpoint.k; }\nb\n";
        let start = source.find("query").unwrap();
        let end = source.find("}\n").unwrap() + 1;
        let range = removal_range(source, start..end);
        let mut result = source.to_owned();
        result.replace_range(range, "");
        assert_eq!(result, "a\nb\n");
    }

    #[test]
    fn unified_diff_has_context_and_counts() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let new = "a\nb\nc\nD\ne\nf\ng\nh\n";
        let diff = unified_diff("x.rs", old, new);
        assert_eq!(
            diff,
            "--- a/x.rs\n+++ b/x.rs\n@@ -1,7 +1,7 @@\n a\n b\n c\n-d\n+D\n e\n f\n g\n"
        );
        assert!(unified_diff("x.rs", old, old).is_empty());
    }
}
