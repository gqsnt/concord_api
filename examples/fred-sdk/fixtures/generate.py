"""Deterministic synthetic FRED v1 fixtures; no provider requests or SDK imports."""
from pathlib import Path
import json
import zipfile

HERE = Path(__file__).parent

def save(name, data):
    (HERE / name).write_text(data, encoding='utf-8')

def js(name, data):
    save(name, json.dumps(data, indent=2) + '\n')

def zipped(name, entries, compression=zipfile.ZIP_DEFLATED):
    with zipfile.ZipFile(HERE / name, 'w') as archive:
        for filename, data in entries:
            info = zipfile.ZipInfo(filename, (2026, 1, 1, 0, 0, 0))
            info.compress_type = compression
            archive.writestr(info, data)

series = dict(id='SYN_A', title='Synthetic output & activity',
              realtime_start='2026-01-01', realtime_end='2026-01-01',
              observation_start='2000-01-01', observation_end='2002-01-01',
              frequency='Annual', frequency_short='A', units='Synthetic units',
              units_short='SU', seasonal_adjustment='Not Seasonally Adjusted',
              seasonal_adjustment_short='NSA', last_updated='2026-01-01 12:00:00-06',
              popularity=7, notes='Authored fixture, no real observations.')
other = dict(series, id='SYN_B', title='Synthetic alternate')
other.pop('notes')

def series_xml(row):
    from xml.sax.saxutils import quoteattr
    return '<series ' + ' '.join(f'{k}={quoteattr(str(v))}' for k, v in row.items()) + '/>'

for i, row in enumerate((series, other)):
    js(f'search-{i}.json', dict(count=2, offset=i, limit=1, seriess=[row]))
    save(f'search-{i}.xml', f'<seriess count="2" offset="{i}" limit="1">\n  {series_xml(row)}\n</seriess>\n')
js('metadata.json', dict(seriess=[series]))
save('metadata.xml', f'<seriess>\n  {series_xml(series)}\n</seriess>\n')

obs = [dict(realtime_start='2026-01-01', realtime_end='2026-01-01', date=date, value=value)
       for date, value in [('2000-01-01', '2.5000'), ('2001-01-01', '.'), ('2002-01-01', 'NA')]]
for offset, rows in ((0, obs[:2]), (2, obs[2:])):
    js(f'observations-{offset}.json', dict(count=3, offset=offset, limit=2, observations=rows))
    xml_rows = ['<observation ' + ' '.join(f'{k}="{v}"' for k, v in row.items()) + '/>' for row in rows]
    save(f'observations-{offset}.xml', f'<observations count="3" offset="{offset}" limit="2">\n  ' + '\n  '.join(xml_rows) + '\n</observations>\n')

csv = 'DATE,SYN_A\n2000-01-01,2.5000\n2001-01-01,.\n2002-01-01,NA\n2003-01-01,\n'
save('observations.csv', csv)
readme = 'Synthetic CSV export for local protocol tests. Missing cells are strings.\n'
save('README.txt', readme)
zipped('observations.zip', [('README.txt', readme), ('SYN_A.csv', csv)])
zipped('stored.zip', [('SYN_A.csv', csv)], zipfile.ZIP_STORED)
zipped('oversized-extracted.zip', [('SYN_A.csv', b'x' * (4 * 1024 * 1024 + 1))])
zipped('oversized-total.zip', [('SYN_A.csv', b'x' * (3 * 1024 * 1024)), ('README.txt', b'x' * (2 * 1024 * 1024))])
zipped('too-many-entries.zip', [(f'{i}.csv', 'x') for i in range(9)])
zipped('ambiguous.zip', [('a.csv', csv), ('b.csv', csv)])
zipped('no-csv.zip', [('README.txt', readme)])
zipped('unsafe-path.zip', [('../SYN_A.csv', csv)])
zipped('unexpected-member.zip', [('SYN_A.csv', csv), ('payload.exe', 'synthetic')])
with zipfile.ZipFile(HERE / 'symlink.zip', 'w') as archive:
    info = zipfile.ZipInfo('SYN_A.csv', (2026, 1, 1, 0, 0, 0))
    info.create_system = 3
    info.external_attr = 0o120777 << 16
    archive.writestr(info, 'outside')
(HERE / 'truncated.zip').write_bytes((HERE / 'observations.zip').read_bytes()[:40])
(HERE / 'malformed.zip').write_bytes(b'PK\x03\x04synthetic invalid central directory')
raw = bytearray((HERE / 'stored.zip').read_bytes())
# Corrupt payload while retaining the original CRC, local and central headers.
payload_at = raw.index(b'DATE,SYN_A')
raw[payload_at] = ord('X')
(HERE / 'bad-crc.zip').write_bytes(raw)
original = bytearray((HERE / 'stored.zip').read_bytes())
central = original.index(b'PK\x01\x02')
encrypted = bytearray(original)
encrypted[6:8] = (1).to_bytes(2, 'little')
encrypted[central+8:central+10] = (1).to_bytes(2, 'little')
(HERE / 'encrypted.zip').write_bytes(encrypted)
unsupported = bytearray(original)
unsupported[8:10] = (99).to_bytes(2, 'little')
unsupported[central+10:central+12] = (99).to_bytes(2, 'little')
(HERE / 'unsupported-method.zip').write_bytes(unsupported)
forged = bytearray(original)
forged[22:26] = (1).to_bytes(4, 'little')
forged[central+24:central+28] = (1).to_bytes(4, 'little')
(HERE / 'forged-size.zip').write_bytes(forged)
js('error.json', dict(error_code=400, error_message='Synthetic key rejected: secret-fred-test-key'))
save('error.xml', '<error code="400" message="Synthetic key rejected: secret-fred-test-key"/>\n')
save('malformed.xml', '<observations count="3" offset="0" limit="2"><observation')
save('malformed.json', '{"count": 3, "observations": [')
save('dtd.xml', '<!DOCTYPE observations [<!ENTITY local "secret-fred-test-key">]><observations count="0" offset="0" limit="1"/>')
