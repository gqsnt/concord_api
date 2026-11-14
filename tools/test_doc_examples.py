"""Documentation extraction must retain whole roots and reject explicit fragments."""
import unittest
from doc_examples import descriptions


class DocumentationExamples(unittest.TestCase):
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
