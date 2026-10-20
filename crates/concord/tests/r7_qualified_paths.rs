#![allow(dead_code)]

mod outer {
    pub trait Encoding {
        type Adapter;
    }
    pub struct ParentAdapter;
    impl Encoding for u32 {
        type Adapter = ParentAdapter;
    }
    impl concord::AtomEncoder<u32> for ParentAdapter {
        fn write(
            _value: &u32,
            writer: &mut concord::BoundedAtomWriter<'_>,
        ) -> Result<(), concord::AtomError> {
            writer.push_str("7")
        }
    }
    pub mod inner {
        pub trait Encoding {
            type Adapter;
        }
        pub struct InnerAdapter;
        impl Encoding for u32 {
            type Adapter = InnerAdapter;
        }
        impl concord::AtomEncoder<u32> for InnerAdapter {
            fn write(
                _value: &u32,
                writer: &mut concord::BoundedAtomWriter<'_>,
            ) -> Result<(), concord::AtomError> {
                writer.push_str("9")
            }
        }
        concord::api! {
            client QualifiedApi {
                base "http://review.example.test/";
                GET read(query count: u32 via <u32 as super::Encoding>::Adapter) -> () via concord::Empty;
                scope nested() {
                    GET read(query count: u32 via <u32 as super::Encoding>::Adapter) -> () via concord::Empty;
                }
            }
        }
    }
}

#[test]
fn r7_qualified_trait_path_keeps_the_authored_parent_binding() {
    let api = outer::inner::QualifiedApi::new().unwrap();
    assert_eq!(
        api.read(0).plan().unwrap().observed_url(),
        "http://review.example.test/?count=7"
    );
    assert!(
        api.nested()
            .read(0)
            .plan()
            .unwrap()
            .observed_url()
            .ends_with("?count=7")
    );
}
