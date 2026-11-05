use concord::reqwest::Client;
use concord_example_github_sdk::{GitHubConfig, GitHubContext, GitHubError, RepoRef};
use concord_example_github_sdk::ids::IssueNumber;
use concord_example_github_sdk::paging::operation;
use concord_example_testserver::{Recorded, Reply, TestServer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind { Accept, Malformed, Mismatch, Foreign }
fn stored(op: &str, repo: &str, parent: &str, url: &str) -> String {
    format!("github-link/1 {op} {repo} {parent} {url}")
}
async fn setup() -> (TestServer, GitHubContext, String) {
    let server = TestServer::start(|_: &Recorded| Reply::json(200,"[]")).await;
    let base = format!("http://api.github.test:{}/api/v3", server.addr().port());
    let http = Client::builder().no_proxy().resolve("api.github.test",server.addr()).build().unwrap();
    let ctx = GitHubContext::new(http,GitHubConfig{token:"synthetic".into()}).with_base_url(&base).unwrap();
    (server, ctx, base)
}

#[tokio::test]
async fn stored_binding_and_url_parser_matrix() {
    use Kind::*;
    let (server,ctx,base)=setup().await;
    let repo=ctx.repo(RepoRef::new("org","repo"));
    let pages=repo.issue_comment_pages(IssueNumber(11),|op|op);
    let url=format!("{base}/repositories/101/issues/11/comments?page=2&cursor=a%2Fb%3D&secret=URL_SENTINEL");
    let valid=stored(operation::ISSUE_COMMENTS,"org/repo","11",&url);
    let mut cases: Vec<(String,String,Kind)>=vec![("canonical".into(),valid.clone(),Accept)];
    for (label,text,expected) in [
        ("empty","".into(),Malformed),
        ("raw URL",url.clone(),Malformed),
        ("version 2",valid.replacen("github-link/1","github-link/2",1),Malformed),
        ("tag case",valid.replacen("github-link","GitHub-link",1),Malformed),
        ("leading space",format!(" {valid}"),Malformed),
        ("trailing space",format!("{valid} "),Malformed),
        ("double delimiter",valid.replacen("/1 ","/1  ",1),Malformed),
        ("tab delimiters",valid.replace(' ',"\t"),Malformed),
        ("newline delimiter",valid.replacen(' ',"\n",1),Malformed),
        ("NBSP delimiter",valid.replacen(' ',"\u{a0}",1),Malformed),
        ("extra field",format!("{valid} extra"),Malformed),
        ("missing URL","github-link/1 issues/list-comments org/repo 11".into(),Malformed),
        ("missing parent",format!("github-link/1 issues/list-comments org/repo {url}"),Malformed),
        ("BOM",format!("\u{feff}{valid}"),Malformed),
    ] {cases.push((label.into(),text,expected));}
    for (parent,expected) in [
        ("11",Accept),("-",Mismatch),("0",Mismatch),("12",Mismatch),
        ("011",Malformed),("+11",Malformed),("-11",Malformed),("-0",Malformed),
        ("11.0",Malformed),("1e1",Malformed),("0xb",Malformed),("١١",Malformed),
        ("18446744073709551615",Mismatch),("18446744073709551616",Malformed),
        ("11\t",Malformed),("\n11",Malformed),("",Malformed),
    ] {cases.push((format!("parent {parent:?}"),stored(operation::ISSUE_COMMENTS,"org/repo",parent,&url),expected));}
    for (name,expected) in [
        ("ORG/REPO",Accept),("Org/RePo",Accept),("other/repo",Mismatch),("org/other",Mismatch),
        ("%6frg/repo",Mismatch),("org/r%65po",Mismatch),("оrg/repo",Mismatch),
        ("/repo",Malformed),("org/",Malformed),("org/repo/extra",Malformed),("orgrepo",Malformed),
        ("org/repo\t",Mismatch),("org\n/repo",Mismatch),("./repo",Mismatch),
    ] {cases.push((format!("binding repo {name:?}"),stored(operation::ISSUE_COMMENTS,name,"11",&url),expected));}
    for op in [operation::ISSUES,operation::PULL_REVIEWS,"gists/list","issues/LIST-comments","issues/list-comments/","","issues/list-comments\t"] {
        cases.push((format!("operation {op:?}"),stored(op,"org/repo","11",&url),Mismatch));
    }
    for (path,expected) in [
        ("/repos/org/repo/issues/11/comments?page=2",Accept),
        ("/repos/org/new-name/issues/11/comments?page=2",Accept),
        ("/repos/other/name/issues/11/comments?page=2",Accept),
        ("/repositories/999/issues/11/comments?page=2",Accept),
        ("/repositories/000101/issues/11/comments?page=2",Accept),
        ("/repositories/no-id/issues/11/comments?page=2",Mismatch),
        ("/repositories/%31%30%31/issues/11/comments?page=2",Mismatch),
        ("/repositories/101/issues/12/comments?page=2",Mismatch),
        ("/repositories/101/pulls/11/reviews?page=2",Mismatch),
        ("/repositories/101/issues/11/comments/1?page=2",Mismatch),
        ("/repositories/101/issues/11/comments/?page=2",Mismatch),
        ("/repositories/101/issues/011/comments?page=2",Mismatch),
        ("/repositories/101/issues/%31%31/comments?page=2",Mismatch),
        ("/repositories/101/%69ssues/11/comments?page=2",Mismatch),
        ("/repositories/101/Issues/11/comments?page=2",Mismatch),
        ("/repositories/101/issues/12/comments?page=2#issues/11/comments",Mismatch),
        ("/repositories/101/issues/12/comments?path=/issues/11/comments",Mismatch),
        ("/repositories/101/issues/11/comments%2F..%2F..%2F12/comments",Mismatch),
        ("/repositories/101/issues/11/./comments?page=2",Accept),
        ("/repositories/101/issues/12/../11/comments?page=2",Accept),
        ("/repositories/101/issues/11/comments?cursor=a%2Fb%3D#ignored",Accept),
        ("/repositories/101/issues/11/comments?cursor=a%2fb%3d%23inside",Accept),
    ] {cases.push((format!("path {path}"),stored(operation::ISSUE_COMMENTS,"org/repo","11",&format!("{base}{path}")),expected));}
    let port=server.addr().port();
    for (label,url,expected) in [
        ("URL host case",format!("http://API.GITHUB.TEST:{port}/api/v3/repositories/101/issues/11/comments"),Accept),
        ("foreign host",format!("http://evil.test:{port}/api/v3/repositories/101/issues/11/comments"),Foreign),
        ("userinfo apparent host",format!("http://api.github.test@evil.test:{port}/api/v3/repositories/101/issues/11/comments"),Foreign),
        ("same-origin userinfo",format!("http://user:pass@api.github.test:{port}/api/v3/repositories/101/issues/11/comments"),Accept),
        ("foreign scheme",format!("https://api.github.test:{port}/api/v3/repositories/101/issues/11/comments"),Foreign),
        ("foreign port","http://api.github.test:1/api/v3/repositories/101/issues/11/comments".into(),Foreign),
        ("foreign base",format!("http://api.github.test:{port}/api/v4/repositories/101/issues/11/comments"),Foreign),
        ("base prefix trick",format!("http://api.github.test:{port}/api/v30/repositories/101/issues/11/comments"),Foreign),
        ("base case",format!("http://api.github.test:{port}/API/v3/repositories/101/issues/11/comments"),Foreign),
        ("relative URL","/api/v3/repositories/101/issues/11/comments".into(),Malformed),
        ("scheme-relative URL",format!("//api.github.test:{port}/api/v3/repositories/101/issues/11/comments"),Malformed),
        ("file URL","file:///api/v3/repositories/101/issues/11/comments".into(),Foreign),
        ("opaque URL","data:text/plain,x".into(),Foreign),
        ("literal URL space",format!("{base}/repositories/101/issues/11/comments?cursor=a b"),Malformed),
        ("escaped URL space",format!("{base}/repositories/101/issues/11/comments?cursor=a%20b"),Accept),
        ("URL trailing LF",format!("{base}/repositories/101/issues/11/comments\n"),Accept),
        ("URL embedded tab",format!("{base}/repositories/101/iss\tues/11/comments"),Accept),
        ("foreign host with tab",format!("http://ev\til.test:{port}/api/v3/repositories/101/issues/11/comments"),Foreign),
    ] {cases.push((label.into(),stored(operation::ISSUE_COMMENTS,"org/repo","11",&url),expected));}
    let mut counts=[0usize;4];
    for (label,text,expected) in &cases {
        let result=pages.restore(text);
        let actual=match &result {
            Ok(_)=>Accept,Err(GitHubError::MalformedStoredLink)=>Malformed,
            Err(GitHubError::ContinuationMismatch{..})=>Mismatch,Err(GitHubError::ForeignLink{..})=>Foreign,
            other=>panic!("unexpected {label}: {other:?}")
        };
        assert_eq!(&actual,expected,"{label}: {result:?}");
        println!("{actual:?}: {label}");
        counts[match actual{Accept=>0,Malformed=>1,Mismatch=>2,Foreign=>3}]+=1;
        match result {
            Ok(key)=>{
                assert_eq!(key.binding(),pages.binding());
                assert_eq!(key.raw_url().fragment(),None);
                let canonical=key.to_stored();
                let roundtrip=pages.restore(&canonical).unwrap();
                assert_eq!(roundtrip,key);
                assert_eq!(roundtrip.to_stored(),canonical);
            },
            Err(error)=>{
                assert!(error.meta().is_none());
                assert!(!format!("{error} {error:?}").contains("URL_SENTINEL"));
            }
        }
    }
    assert!(server.requests().is_empty(),"all restores are without I/O");
    println!("stored parser: {} cases; accept/malformed/mismatch/foreign={counts:?}; every accepted key has stable canonical round-trip",cases.len());
    server.shutdown().await;
}

#[tokio::test]
async fn top_level_and_u64_parent_boundaries_round_trip() {
    let (server,ctx,base)=setup().await;
    let repo=ctx.repo(RepoRef::new("org","repo"));
    let issues=repo.issue_pages(repo.issues(&Default::default()));
    let url=format!("{base}/repositories/101/issues?page=2");
    let key=issues.restore(&stored(operation::ISSUES,"org/repo","-",&url)).unwrap();
    assert_eq!(key.binding().parent(),None);
    assert_eq!(issues.restore(&key.to_stored()).unwrap(),key);
    for p in ["0","11"] {
        assert!(matches!(issues.restore(&stored(operation::ISSUES,"org/repo",p,&url)),Err(GitHubError::ContinuationMismatch{..})));
    }
    for n in [0,u64::MAX] {
        let pages=repo.issue_comment_pages(IssueNumber(n),|op|op);
        let text=stored(operation::ISSUE_COMMENTS,"org/repo",&n.to_string(),&format!("{base}/repositories/101/issues/{n}/comments?page=2"));
        let key=pages.restore(&text).unwrap();
        assert_eq!(key.binding().parent(),Some(n));
        assert_eq!(pages.restore(&key.to_stored()).unwrap(),key);
    }
    let unknown=issues.restore(&stored("unknown-operation","org/repo","-",&url));
    assert!(matches!(unknown,Err(GitHubError::ContinuationMismatch{found,..}) if found.operation()==operation::UNRECOGNIZED));
    assert!(server.requests().is_empty());
    println!("top-level '-' and numeric u64 boundaries round-trip; wrong parent and unknown operation rejected");
    server.shutdown().await;
}

#[tokio::test]
async fn operation_mismatches_keep_known_ids_and_redact_unknown_ids() {
    let (server,ctx,base)=setup().await;
    let repo=ctx.repo(RepoRef::new("org","repo"));
    let pages=repo.issue_comment_pages(IssueNumber(11),|op|op);
    let url=format!("{base}/repositories/101/issues/11/comments?page=2");
    let listings=[operation::ISSUES,operation::ISSUE_COMMENTS,operation::REPO_COMMENTS,operation::LABELS,operation::PULLS,operation::PULL_REVIEWS,operation::PULL_FILES,operation::WORKFLOWS,operation::RUNS,operation::WORKFLOW_RUNS,operation::RUN_JOBS,operation::ARTIFACTS,operation::RUN_ARTIFACTS];
    for id in listings {
        let result=pages.restore(&stored(id,"org/repo","11",&url));
        if id==operation::ISSUE_COMMENTS {assert!(result.is_ok());}
        else {assert!(matches!(result,Err(GitHubError::ContinuationMismatch{found,..}) if found.operation()==id));}
    }
    for id in ["",operation::UNRECOGNIZED,"not-an-operation","ISSUES/list-comments","issues/list-comments\n"] {
        let error=pages.restore(&stored(id,"org/repo","11",&url)).unwrap_err();
        assert!(matches!(error,GitHubError::ContinuationMismatch{ref found,..} if found.operation()==operation::UNRECOGNIZED));
        assert!(!format!("{error} {error:?}").contains("not-an-operation"));
    }
    assert!(server.requests().is_empty());
    println!("13 declared operation IDs: own accepted, all other known IDs mismatched accurately; unknown/empty/case/newline IDs refused as unrecognized, no I/O");
    server.shutdown().await;
}
