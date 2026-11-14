"""Regression checks for the safety inventory and isolated-manifest builder."""

import unittest

from panics import findings


class Inventory(unittest.TestCase):
    def kinds(self, source):
        return {finding.kind for finding in findings(source)}

    def test_comments_literals_and_debug_assert_are_not_findings(self):
        self.assertEqual(self.kinds('''
// panic!("wrong");
/* /* nested */ x[9].unwrap() */
const TEXT: &str = r###"expect(\"x\"); panic!(); a[3]"###;
fn okay() { debug_assert!(true); }
'''), set())

    def test_test_only_items_are_removed_without_hiding_production(self):
        rows = list(findings('''
#[cfg(test)]
mod tests { fn boom() { panic!("test"); } }
fn production() { value.unwrap(); }
#[cfg(test)]
fn unit() { value.expect("test"); }
fn after() { assert!(flag); }
'''))
        self.assertEqual([row.kind for row in rows], ["unwrap", "assert!"])

    def test_all_requested_hazards_and_lint_exceptions_are_recorded(self):
        self.assertEqual(self.kinds('''
#[allow(clippy::panic)]
fn first() { panic!("bad"); }
#[expect(clippy::unwrap_used)]
fn second() {
 x.unwrap(); y.expect("bad"); unreachable!(); assert!(ok);
 let a = values[at];
 let b = &text[start..end];
 let c = size + 1;
 let d = value as u8;
 let e = bits >> shift;
}
'''), {"lint-exception", "panic!", "unwrap", "expect", "unreachable!", "assert!", "index-or-slice", "arithmetic", "numeric-cast", "shift"})

    def test_reason_must_touch_the_operation(self):
        rows = list(findings('''
fn run() {
 // PANIC-SAFETY: the branch checked the bound.
 let first = bytes[0];
 let second = bytes[1];
}
'''))
        self.assertTrue(rows[0].reason)
        self.assertFalse(rows[1].reason)

    def test_multiplication_is_not_hidden_by_operand_names(self):
        self.assertIn("arithmetic", self.kinds("fn f() { let x = count * self.size; }"))
        self.assertNotIn("arithmetic", self.kinds("fn f() { let x = *pointer; }"))

    def test_macro_star_repetition_is_not_arithmetic(self):
        self.assertEqual(self.kinds('''
macro_rules! m { ($($x:tt)*) => { $(helper!($x);)* fn after() {} }; }
'''), set())

    def test_macro_comma_repetition_is_not_arithmetic(self):
        self.assertEqual(self.kinds('''
macro_rules! m { ($($x:tt),*) => { consume!($($x),*); }; }
'''), set())

    def test_macro_plus_repetition_is_not_arithmetic(self):
        self.assertEqual(self.kinds('''
macro_rules! m { ($($x:tt)+) => { $(helper!($x);)+ fn after() {} }; }
'''), set())

    def test_macro_repeated_token_group_is_not_indexing(self):
        self.assertEqual(self.kinds('''
macro_rules! m { (@docs [$($x:tt)*]) => { helper!(@docs [$($x)*]); }; }
'''), set())

    def test_macro_token_tree_fragment_is_not_indexing(self):
        self.assertEqual(self.kinds('''
macro_rules! m { ($x:tt [$($tail:tt)*]) => { $x }; }
'''), set())

    def test_macro_nested_optional_and_path_repetitions_are_syntax(self):
        self.assertEqual(self.kinds('''
macro_rules! m {
    ($($path:ident)::+ $(, [$($x:tt)*])?) => {
        $( $(helper!($x);)* )? fn after() {}
    };
}
'''), set())

    def test_macro_arithmetic_token_separators_are_syntax(self):
        for separator, operator in (("-", "*"), ("/", "+"), ("%", "*"), ("*=", "+")):
            with self.subTest(separator=separator):
                self.assertEqual(self.kinds(
                    'macro_rules! m { ($($x:tt)*) => { consume!($($x)'
                    + separator + operator + '); }; }'
                ), set())

    def test_macro_body_still_reports_arithmetic_and_indexing(self):
        rows = list(findings('''
macro_rules! m {
    ($($x:expr),*) => { $(
        let product = ($x) * count;
        let next = $x + 1;
        let value = values[$x];
        let repeated_index = values[$($x)*];
        let slice = &values[..$x];
    )* };
}
'''))
        self.assertEqual([row.kind for row in rows], [
            "arithmetic", "arithmetic", "index-or-slice", "index-or-slice", "index-or-slice",
        ])

    def test_macro_helper_groups_still_report_hazards_inside(self):
        self.assertEqual(self.kinds('''
macro_rules! m { ($x:tt) => { helper!(@docs [values[at] + 1]); }; }
'''), {"index-or-slice", "arithmetic"})


if __name__ == "__main__":
    unittest.main()
