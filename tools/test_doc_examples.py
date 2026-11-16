"""Documentation extraction must retain whole roots and reject explicit fragments."""
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch
from doc_examples import descriptions, expected


class DocumentationExamples(unittest.TestCase):
    def expected_example(self, page):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            for name, text in [('README.md', ''), ('docs/reference/overview.md', ''),
                               ('docs/examples/example.md', page)]:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            with patch('doc_examples.ROOT', root):
                return expected()

    def test_example_descriptions_get_separate_sources_in_page_order(self):
        first = 'use concord::Json;\nconcord::api! { pub client Example { read Json; } }\n'
        second = 'use concord::Body;\nconcord::api! { pub client Uploads { POST put(bytes: Body); } }\n'
        third = 'concord::api! { pub client Third { GET get(); } }\n'
        for sources in [(first, second), (first, second, third)]:
            with self.subTest(descriptions=len(sources)):
                # Helpers and fragments do not consume description numbers.
                blocks = [first, 'fn helper() {}\n', 'client Fragment { … }\n', *sources[1:]]
                contents, inventory = self.expected_example(
                    '\n'.join(f'```rust\n{block}```\n' for block in blocks))
                self.assertEqual(len(inventory), len(sources))
                for number, source in enumerate(sources, 1):
                    module = 'description' if number == 1 else f'description_{number:03}'
                    compiled = f'tests/real-apis/src/example/{module}.rs'
                    case = inventory[number - 1]
                    self.assertEqual(case['compiled'], compiled)
                    self.assertEqual(case['block'], 1 if number == 1 else number + 2)
                    self.assertEqual(contents[compiled], source)
                    self.assertEqual(contents[case['fixture']], next(descriptions(source)))
                    self.assertIn(f'crate::example::{case["client"]}',
                                  contents['tests/real-apis/src/guide/mod.rs'])

    def test_example_descriptions_in_one_block_need_separate_blocks(self):
        with self.assertRaisesRegex(ValueError, r'example.md: block 1: .*separate fenced blocks'):
            self.expected_example('```rust\n'
                                  'concord::api! { client A { GET get(); } }\n'
                                  'concord::api! { client B { POST post(); } }\n```\n')

    def test_literals_and_comments_do_not_close_the_description(self):
        source = '''use concord::Json;
concord::api! {
    pub client Example {
        base "https://example.test/{not_a_delimiter}";
        // A closing brace in a comment: }
        GET get() { body r###"{"key":"}"}"###; }
    }
}
'''
        roots = list(descriptions(source))
        self.assertEqual(len(roots), 1)
        self.assertTrue(roots[0].startswith('pub client Example'))
        self.assertIn('body r###"{"key":"}"}"###;', roots[0])
        self.assertTrue(roots[0].endswith('}\n'))

    def test_each_whole_macro_in_one_block_is_kept(self):
        roots = list(descriptions('concord::api! { client A { GET get(); } }\n'
                                  'concord::api! { client B { POST post(); } }'))
        self.assertEqual(roots, ['client A { GET get(); }\n', 'client B { POST post(); }\n'])

    def test_explicit_omissions_and_endpoint_fragments_are_not_whole_descriptions(self):
        for source in ['pub client Example { … }', 'GET get();',
                       'pub client Example { // base, read, endpoints …\n}',
                       'client Example {']:
            self.assertEqual(list(descriptions(source)), [])

    def test_a_fragment_does_not_hide_whole_descriptions_in_the_same_block(self):
        source = ('concord::api! { client Fragment { … } }\n'
                  'concord::api! { client Whole { GET get(); } }\n'
                  'client Bare { POST post(); }')
        self.assertEqual(list(descriptions(source)),
                         ['client Whole { GET get(); }\n', 'client Bare { POST post(); }\n'])
