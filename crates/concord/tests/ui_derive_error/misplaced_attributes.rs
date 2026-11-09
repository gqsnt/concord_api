#[derive(Debug, concord::Error)]
#[display(with = fmt_error)]
pub enum Hooked {
    #[display("{0}")]
    Gated(u32),
}

fn fmt_error(_: &Hooked, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("hooked")
}

#[derive(Debug, concord::Error)]
#[display("{0}")]
pub enum OnEnum {
    #[display(with = fmt_error)]
    Gated(u32),
    #[display("{error}")]
    #[from(skip)]
    Concord { error: concord::Error },
}

fn main() {}
