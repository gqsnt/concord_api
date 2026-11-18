//! A path of a status arm that names an application's `__check_problem_1::Witness` resolves to
//! the application's type, never to one of the check.
use std::any::TypeId;
mod __check_problem_1 {
    pub struct Witness;
}
fn is_application_type<T: 'static>(_: T) -> bool {
    TypeId::of::<T>() == TypeId::of::<crate::__check_problem_1::Witness>()
}
concord::api! {
    client Who {
        base "https://unused.invalid";
        GET check() -> bool {
            status { 2xx => is_application_type(__check_problem_1::Witness) }
        }
    }
}
fn main() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let client = Who::new().with_transport(concord::fake(|_| Ok(concord::reply(200, ""))));
            assert!(client.check().await.unwrap(), "the arm saw another Witness");
        });
}
