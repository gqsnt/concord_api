"""C02-B replayed on the 4.2 candidate: generated prose must keep the literal tokens of the signature types."""
from html.parser import HTMLParser
from pathlib import Path
import hashlib, json, sys
class Text(HTMLParser):
    def __init__(self): super().__init__(); self.parts = []
    def handle_data(self, d): self.parts.append(d)
doc = Path(sys.argv[1])
cases = [
    ('struct.CurrentOrdinary.html', 'Output: [u8; { "a < b".len() }]', '"a<b"', 'pub async fn send(self) -> Result<[u8; 5], Error>'),
    ('struct.CurrentRaw.html', 'Output: [u8; { r#"p :: q"#.len() }]', 'r#"p::q"#', 'pub async fn send(self) -> Result<[u8; 6], Error>'),
    ('struct.Current.html', 'Body (item): [u8; { "x , y".len() }]', '"x, y"', 'pub fn comma(&self, item: impl IntoValue<[u8; 5]>)'),
]
out = []
for page, prose, altered, signature in cases:
    html = (doc / page).read_text(); p = Text(); p.feed(html); text = ''.join(p.parts)
    ok = prose in text and altered not in text and signature in text
    out.append({'page': page, 'faithful_prose_present': prose in text, 'altered_absent': altered not in text,
                'signature_control': signature in text, 'html_sha256': hashlib.sha256(html.encode()).hexdigest(), 'pass': ok})
print(json.dumps(out, indent=2)); sys.exit(0 if all(c['pass'] for c in out) else 1)
