mod caller {
    #[derive(Clone)] struct Number(u32);
    type Output = u32;
    concord::api! { client Local(input:Number) {base "https://types.test";
        query {"input"=input.0;} read concord::Json;
        scope nested(number:Number) {query {"number"=number.0;}
            GET get(value:Number) -> Output {query {"value"=value.0;}}
        }
    } }
    pub fn check() {let _=Local::new(Number(1)).nested(Number(2)).get(Number(3)).request().unwrap();}
}
fn main(){caller::check();}
