use concord_pagination::{Budget, Options, JobOptions, Resume, Page, Manifest, Outcome, ItemCheckpoint, JobsError, independent, manifest_jobs, sequential, resume_items};
use std::{cell::{Cell, RefCell}, num::NonZeroUsize};
fn budget(n:usize)->Budget{Budget::new(NonZeroUsize::new(n).unwrap())}
#[derive(Debug)] struct Failure { resume: Resume<u32> }
async fn internal_chain(start:u32)->Result<(),Failure>{
    let mut pages=sequential(Resume::At(start),budget(9),|key|async move{
        match key {Some(10)=>Ok(Page::<u32,u32>::new(vec![1],Some(20))),_=>Err(Failure{resume:Resume::At(20)})}
    });
    while let Some(page)=pages.next().await {page?;}
    Ok(())
}
#[tokio::test]
async fn report_keys_are_job_starts_and_later_resume_is_in_error(){
    let mut jobs=independent([10],Options::new(budget(9)),internal_chain);
    let error=jobs.next().await.unwrap().unwrap_err();
    assert_eq!(error.key,10);assert_eq!(error.value.resume,Resume::At(20));
    let report=jobs.finish();
    assert_eq!(report.failed().copied().collect::<Vec<_>>(),[10]);
    assert_eq!(report.into_rerun().1.collect::<Vec<_>>(),[10]);
    let mut jobs=manifest_jobs(Resume::<u32>::Start,JobOptions::new(budget(9),Options::new(budget(9))),
        |_|async{Ok::<_,()>(Manifest::new(vec![10],None))},internal_chain);
    match jobs.next().await.unwrap(){
        Err(JobsError::Job(error))=>{assert_eq!(error.key,10);assert_eq!(error.value.resume,Resume::At(20));},
        other=>panic!("{other:?}"),
    }
    let report=jobs.finish();
    assert_eq!(report.failed().copied().collect::<Vec<_>>(),[10]);
    assert_eq!(report.into_rerun().1,[10]);
    println!("IndependentReport and JobsReport failed()/into_rerun(): start=10; emitted error carries actual internal restart=20");
}
struct Meta<'a>(&'a Cell<usize>);
impl Drop for Meta<'_>{fn drop(&mut self){self.0.set(self.0.get()+1)}}
#[tokio::test]
async fn replay_verification_runs_in_factory_before_items_discards_prefix_and_metadata(){
    let dropped=Cell::new(0);
    let mut items=sequential(Resume::Start,budget(5),|_:Option<u32>|async{
        Ok::<_,&str>(Page{items:vec![11,12,13],next:None,meta:Meta(&dropped)})
    }).items();
    assert_eq!(items.next().await.unwrap().unwrap(),11);
    assert_eq!(dropped.get(),1,"Items has already dropped page metadata");
    let checkpoint=items.checkpoint().cloned().unwrap();assert_eq!(checkpoint.skip,1);
    drop(items);
    let verified=Cell::new(false);
    let mut resumed=resume_items(checkpoint.clone(),budget(5),|_:Option<u32>|async{
        let page=Page::<u32,u32>::new(vec![99,12,13],None);
        verified.set(true);
        if page.items.get(checkpoint.skip-1)!=Some(&11){Err("stale") }else{Ok(page)}
    });
    assert_eq!(resumed.next().await,Some(Err("stale")));assert!(verified.get());
    assert_eq!(resumed.finish().outcome,Outcome::Failed);
    let mut unchecked=resume_items(ItemCheckpoint{page:Resume::Start,skip:1},budget(5),|_:Option<u32>|async{Ok::<_,&str>(Page::<u32,u32>::new(vec![99,12,13],None))});
    assert_eq!(unchecked.next().await,Some(Ok(12)));
    println!("Items drops metadata; factory verifies skipped item before projection; stale replay fails there; unchecked replay silently skips changed prefix");
}
#[tokio::test]
async fn budget_ends_two_key_cycle_without_comparing_keys(){
    let seen=RefCell::new(Vec::new());
    let mut chain=sequential(Resume::At('A'),budget(5),|key|{
        seen.borrow_mut().push(key.unwrap());
        async move{Ok::<_,()>(Page::<(),char>::new(vec![],Some(if key==Some('A'){'B'}else{'A'})))}
    });
    while let Some(p)=chain.next().await{p.unwrap();}
    assert_eq!(chain.finish().outcome,Outcome::Bounded);
    assert_eq!(*seen.borrow(),['A','B','A','B','A']);
    println!("Budget: A -> B -> A -> B -> A, exactly 5 charged calls, Outcome::Bounded");
}
