"""Create new semantic fixtures; deliberately never overwrite expected output."""
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2] / 'target' / 'semantic-fixtures'
CASES = []
def case(n, name, source):
    CASES.append((n,name,source))
def api(n,name,content):
    case(n,name,'client Workshop { base "https://workshop.test"; read Reader; '+content+' }')
case(1,'siblings','client Workshop { GET fetch(); POST fetch(); }')
case(1,'parts','client Workshop { part common() {} part common() {} GET fetch(..common); }')
api(1,'inputs','GET fetch(code: u32, code: u32) { query { code; } }')
api(2,'optional_child','scope records(fetch?: u8) { query { fetch; } GET fetch(); }')
api(3,'optional_reserved','GET fetch(request?: String) { query { request; } }')
api(3,'child_reserved','GET new();')
api(3,'prefix_optional','GET fetch(with_budget?: u8) { query { with_budget; } }')
api(3,'prefix_child','GET with_budget();')
api(3,'alias_error','scope error { GET fetch(); }')
api(3,'alias_result','GET result();')
for kw in ['match','r#loop']:
    api(4,'input_'+kw.replace('#',''),f'GET fetch({kw}: String) {{ query {{ {kw}; }} }}')
api(4,'scope_keyword','scope r#mod { GET fetch(); }')
api(4,'endpoint_keyword','GET r#fn();')
for place, declaration in [('scope','scope Bad__Name { GET fetch(); }'),('endpoint','GET fetch__record();'),('input','GET fetch(Bad__Name: u8) { query { "wire" = Bad__Name; } }')]: api(5,place,declaration)
for client in ['workshop','Self','Match','Workshop_Bad']:
    case(6,client.lower(),f'client {client} {{ base "https://names.test"; GET fetch(); }}')
api(7,'owner_scope','scope client { GET fetch(); }')
api(7,'repeated_owner','scope outer { scope outer { GET fetch(); } }')
api(7,'input_owner','scope outer { GET fetch(outer: String) { query { outer; } } }')
api(7,'input_client','GET fetch(client: String) { query { client; } }')
api(8,'generated_type','GET sha2(); GET sha_2();')
api(9,'unused','GET fetch(spare: u16);')
for name,ty in [('str','&str'),('slice','&[u8]'),('nested','Vec<&str>')]: api(10,'borrow_'+name,f'GET fetch(label: {ty}) {{ query {{ label; }} }}')
api(10,'self_input','GET fetch(label: Vec<Self>) { query { label; } }')
api(10,'self_output','GET fetch() -> Vec<Self>;')
case(10,'self_problem','client Workshop { problem Vec<Self> read Reader; GET fetch(); }')
api(10,'self_expression','GET fetch() { body { let _: Self = make(); make() }; }')
api(11,'hole','GET fetch() { path "records/{missing_code}"; }')
api(11,'short','GET fetch() { query { missing_code; } }')
api(11,'base','GET fetch() { base missing_base; }')
api(11,'part_scope','part route() { headers { "x-region" = "{outer.region}"; } } scope outer(region: String) { path "{region}"; GET fetch(..route); }')
case(12,'ancestor_hole','client Workshop(region: String) { base "https://regions.test"; GET fetch() { path "{region}"; } }')
case(12,'ancestor_base','client Workshop(server: Base) { GET fetch() { base server; } }')
case(12,'ancestor_short','client Workshop(region: String) { base "https://regions.test"; GET fetch() { query { region; } } }')
case(12,'qualified_short','client Workshop(region: String) { base "https://regions.test"; GET fetch() { query { client.region; } } }')
api(13,'path','GET fetch(code?: String) { path "records/{code}"; }')
api(13,'host','GET fetch(zone?: String) { base "https://{zone}.workshop.test"; }')
api(13,'base','GET fetch(server?: Base) { base server; }')
api(14,'input','GET fetch(#[serde(default)] code: u32) { query { code; } }')
api(14,'part','#[cfg(feature="extra")] part fragment() {} GET fetch(..fragment);')
case(14,'client','#[deprecated] client Workshop { GET fetch(); }')
api(14,'scope','#[repr(C)] scope fragment { GET fetch(); }')
api(14,'endpoint','#[inline] GET fetch();')
api(14,'statement','GET fetch() { #[doc="why"] path "records"; }')
api(15,'method','fetch();')
case(16,'after_child','client Workshop { GET fetch(); read Reader; }')
case(16,'after_part','client Workshop { part fragment() {} read Reader; GET fetch(..fragment); }')
case(16,'part_after_child','client Workshop { GET fetch(); part fragment() {} }')
for word,stmt in [('base','base "https://duplicate.test";'),('path','path "records";'),('query','query { "q" = 1; }'),('headers','headers { "x-q" = 1; }'),('body','body make();'),('read','read Reader;'),('status','status { 410 => () }'),('paginate','paginate pager;'),('idempotent','idempotent;'),('idle_timeout','idle_timeout clock();')]:
    case(17,'duplicate_'+word,'client Workshop { GET fetch() { '+stmt+' '+stmt+' } }')
case(17,'duplicate_problem','client Workshop { problem Issue read Reader; problem Other read Reader; }')
case(17,'inside_part','client Workshop { part fragment() { read First; read Second; } GET fetch(..fragment); }')
for word,stmt in [('body','body make();'),('paginate','paginate pager;'),('idempotent','idempotent;')]:
    for place in ['client','scope','spread']:
        if place=='client': case(18,word+'_'+place,'client Workshop { '+stmt+' }')
        if place=='scope': case(18,word+'_'+place,'client Workshop { scope fragment { '+stmt+' } }')
        if place=='spread': api(18,word+'_'+place,'part fragment() { '+stmt+' } scope branch(..fragment) { GET fetch(); }')
api(19,'method','METHOD "BAD/METHOD" fetch();')
for word,stmt in [('problem','problem Issue;'),('middleware','middleware Layer;'),('part','part nested() {}')]:
    for place in ['scope','endpoint']:
        api(20,word+'_'+place,('scope fragment { ' if place=='scope' else 'GET fetch() { ')+stmt+' }')
api(21,'header','headers { "x-mode" = "one"; } GET fetch() { headers { "X-Mode" = "two"; } }')
api(21,'query','query { "mode" = 1; } GET fetch() { query { "mode" = 2; } }')
api(21,'arm','status { 410 => () } GET fetch() { status { 410 => () } }')
api(21,'class_member','status { 4xx => () } GET fetch() { status { 410 => () } }')
for kind,key in [('headers','x-mode'),('query','mode'),('body make','mode')]:
    api(22,kind.replace(' ','_'),f'GET fetch() {{ {kind} {{ override "{key}" = 1; }} }}')
api(22,'arm','GET fetch() { status { override 410 => () } }')
api(22,'after_removal','query { "mode" = 1; } scope s { query { remove "mode"; } GET fetch() { query { override "mode" = 2; } } }')
for kind,key in [('headers','x-mode'),('query','mode'),('body make','mode')]: api(23,kind.replace(' ','_'),f'GET fetch() {{ {kind} {{ remove "{key}"; }} }}')
api(23,'arm','GET fetch() { status { remove 410 } }')
api(23,'class_member','status { 4xx => () } GET fetch() { status { remove 410 } }')
for word,stmt in [('base','base "https://edits.test";'),('path','path "records";'),('query','query {}'),('headers','headers {}'),('status','status {}'),('body','body make();'),('read','read Reader;'),('paginate','paginate pager;'),('idempotent','idempotent;'),('idle_timeout','idle_timeout clock();'),('problem','problem Issue read Reader;'),('middleware','middleware Layer;')]:
    for edit in ['override','remove']:
        if word in ['problem','middleware']: case(24,word+'_'+edit,'client Workshop { '+edit+' '+stmt+' }')
        else: api(24,word+'_'+edit,'GET fetch() { '+edit+' '+stmt+' }')
for name,path in [('empty',''),('leading','/records'),('doubled','records//meta')]: api(25,name,f'GET fetch() {{ path "{path}"; }}')
for name,path in [('dot','records/./meta'),('dotdot','records/../meta')]: api(26,name,f'GET fetch() {{ path "{path}"; }}')
for name,path in [('space','record data'),('hash','record#data'),('percent','record%20data'),('backslash',r'record\\data'),('tab',r'record\tdata'),('newline',r'record\ndata'),('del',r'record\x7fdata')]: api(27,name,f'GET fetch() {{ path "{path}"; }}')
for name,path in [('base_backslash',r'https://base.test/a\\b'),('base_control',r'https://base.test/a\tb'),('base_space','https://base.test/a b')]: case(27,name,f'client Workshop {{ base "{path}"; GET fetch(); }}')
api(28,'query','GET fetch() { path "records?sort=new"; }')
api(29,'suffix','GET fetch(key: String) { path "objects/{*key}.bin"; }')
api(29,'prefix','GET fetch(key: String) { path "objects/pre{*key}"; }')
api(29,'base','GET fetch(zone: String) { base "https://{*zone}.workshop.test"; }')
for kind in ['query','headers','body make']: api(29,kind.replace(' ','_'),f'GET fetch(code: String) {{ {kind} {{ "x-code" = "{{*code}}"; }} }}')
for name,key in [('space','bad key'),('empty',''),('unicode','naïve'),('colon','bad:key')]: api(30,name,f'GET fetch() {{ headers {{ "{key}" = "v"; }} }}')
api(31,'head','HEAD fetch() -> bool { status { 2xx { body } => true } }')
api(31,'part','part answer() { status { 201 { body } => true } } HEAD fetch(..answer) -> bool;')
api(32,'head','HEAD fetch() -> u8;')
case(33,'arrow','client Workshop { base "https://workshop.test"; GET fetch() -> Record; }')
case(33,'arm','client Workshop { base "https://workshop.test"; GET fetch() -> Record { status { 2xx { body } => body } } }')
case(33,'problem','client Workshop { base "https://workshop.test"; problem Issue; GET fetch(); }')
api(34,'field','GET fetch() { status { 201 { payload } => () } }')
api(34,'duplicate_field','GET fetch() { status { 201 { headers, headers: other } => () } }')
api(34,'duplicate_binding','GET fetch() { status { 201 { headers: both, status: both } => () } }')
api(35,'unknown','GET fetch(..not_declared);')
api(36,'unused','part spare() {} GET fetch();')
for word,stmt in [('path','path "records";'),('problem','problem Issue;'),('middleware','middleware Layer;'),('child','GET nested();'),('scope','scope nested {}')]: api(37,word,'part fragment() { '+stmt+' } GET fetch(..fragment);')
case(37,'part_spread','client Workshop { part one(..two) {} part two() {} GET fetch(..one); }')
case(37,'client_spread','client Workshop(..fragment) { part fragment() {} GET fetch(); }')
for word,stmt in [('base','base "https://part.test";'),('read','read Reader;'),('body','body make();'),('paginate','paginate pager;'),('idempotent','idempotent;'),('idle_timeout','idle_timeout clock();')]:
    api(38,word+'_node','part one() { '+stmt+' } GET fetch(..one) { '+stmt+' }')
    api(38,word+'_parts','part one() { '+stmt+' } part two() { '+stmt+' } GET fetch(..one,..two);')
for kind,key in [('query','q'),('headers','x-q'),('body make','q')]:
    for where in ['node','parts','block']:
        stmt=f'{kind} {{ "{key}" = 1; }}'
        source={'node':'part one() { '+stmt+' } GET fetch(..one) { '+stmt+' }','parts':'part one() { '+stmt+' } part two() { '+stmt+' } GET fetch(..one,..two);','block':f'GET fetch() {{ {kind} {{ "{key}" = 1; "{key}" = 2; }} }}'}[where]
        api(38,kind.replace(' ','_')+'_'+where,source)
for where in ['node','parts','block']:
    source={'node':'part one() { status { 410 => () } } GET fetch(..one) { status { 410 => () } }','parts':'part one() { status { 410 => () } } part two() { status { 410 => () } } GET fetch(..one,..two);','block':'GET fetch() { status { 410 => (), 410 => () } }'}[where]
    api(38,'arm_'+where,source)
api(38,'input_node','part one(code: u32) { query { code; } } GET fetch(code: u32,..one) { query { code; } }')
api(38,'input_parts','part one(code: u32) { query { code; } } part two(code: u32) { query { code; } } GET fetch(..one,..two);')
api(38,'twice','part one() {} GET fetch(..one,..one);')
api(38,'body_functions','part one() { body make { "a" = 1; } } POST fetch(..one) { body other { "b" = 2; } }')
api(39,'no_arrow','status { 2xx { body } => unpack(body) } DELETE fetch();')
case(40,'optional','client Workshop(rate?: u16) { base "https://workshop.test"; middleware Quota(rate); GET fetch(); }')
case(40,'qualified','client Workshop(rate?: u16) { base "https://workshop.test"; middleware Quota(client.rate); GET fetch(); }')
api(41,'short_header','GET fetch(trace_id: String) { headers { trace_id; } }')
for key in ['HOST','Host','host']: api(42,key,'GET fetch() { headers { "'+key+'" = "host.test"; } }')
case(43,'no_base','client Workshop { GET fetch(); }')
api(44,'class_override','status { 202 => () } GET fetch() { status { override 2xx => () } }')
for key in ['601','099','6xx','2XX','20x','200u16','2_00','0xc8']:
    api(45,key,'GET fetch() { status { '+key+' => () } }')
for wrapper in ['Option<Record>','Result<Record,Issue>']:
    for key in ['2xx','202']:
        api(46,wrapper.split('<')[0].lower()+'_'+key,'status { '+key+' { body } => unpack(body) } GET fetch() -> '+wrapper+';')
case(43,'multiple','client Workshop { GET fetch(unused: &str, id?: u32) { path "/{id}"; headers { "Host" = "v"; "bad key" = "v"; } status { 601 => () } } GET other(); }')

# C1.6 is a closed list: each receiver/name pair has a compiler snapshot.
for receiver,names in [('call',['send','send_full','send_raw','send_if_none_match','request','into_request','edit_request','read_response','pages','at_url','into_future','clone','with_extra']),('client',['call','clone','new','with_extra']),('handle',['call','clone','with_extra'])]:
    for method in names:
        if receiver=='call': api(3,'reserved_'+receiver+'_'+method,f'GET fetch({method}?: String) {{ query {{ {method}; }} }}')
        elif receiver=='client': case(3,'reserved_'+receiver+'_'+method,f'client Workshop({method}?: String) {{ base "https://names.test"; query {{ {method}; }} GET fetch(); }}')
        else: api(3,'reserved_'+receiver+'_'+method,f'scope branch({method}?: String) {{ query {{ {method}; }} GET fetch(); }}')
for receiver,names in [('client',['call','clone','new','with_extra']),('handle',['call','clone','with_extra'])]:
    for method in names:
        decl=f'GET {method}();'
        api(3,'child_'+receiver+'_'+method,decl if receiver=='client' else 'scope branch { '+decl+' }')
for keyword in 'abstract as async await become box break const continue crate do dyn else enum extern false final fn for gen if impl in let loop macro match mod move mut override priv pub ref return self static struct super trait true try type typeof unsafe unsized use virtual where while yield'.split():
    api(4,'keyword_'+keyword,f'GET fetch({keyword}: String) {{ query {{ {keyword}; }} }}')
for attribute in ['cfg(feature="extra")','deprecated','doc(hidden)','repr(C)']:
    tag=attribute.split('(')[0]
    api(14,'input_'+tag,f'GET fetch(#[{attribute}] code: u32) {{ query {{ code; }} }}')
for path,name in [('a//b','base_empty'),('a/../b','base_dot'),('a/./b','base_single_dot')]:
    case(25 if '//' in path else 26,name,f'client Workshop {{ base "https://base.test/{path}"; GET fetch(); }}')
case(39,'own_part','client Workshop { base "https://part.test"; read Reader; part answer() { status { 202 { body } => unpack(body) } } GET fetch(..answer); }')
case(46,'scope_arm','client Workshop { base "https://part.test"; read Reader; scope branch { status { 202 { body } => unpack(body) } GET fetch() -> Option<Record>; } }')
api(4,'part_keyword','part r#loop() {} GET fetch(..r#loop);')
api(5,'part_name','part Bad__Part() {} GET fetch(..Bad__Part);')
api(10,'self_body_function','POST fetch() { body <Self as Maker>::make { "key" = 1; } }')
api(21,'short_query','query { "code" = 1; } GET fetch(code: u32) { query { code; } }')
api(29,'beside_hole','GET fetch(prefix: String, rest: String) { path "{prefix}{*rest}"; }')
ROOT.mkdir(parents=True, exist_ok=True)
for n,name,src in CASES:
    (ROOT/f'{n:02}_{name}.rs').write_text('concord::api! {\n    '+src+'\n}\nfn main() {}\n')
(ROOT/'coverage.tsv').write_text(''.join(f'{n:02}_{name}.rs\t{n}\n' for n,name,_ in CASES))
print(len(CASES),'cases')
