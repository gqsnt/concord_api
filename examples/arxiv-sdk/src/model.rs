/// One Atom result page, with OpenSearch paging metadata.
#[derive(Clone, Debug)]
pub struct Feed {
    /// Total matches reported by the provider; not a snapshot guarantee.
    pub total_results: u64,
    /// Zero-based start index.
    pub start: u64,
    /// Provider's reported page size.
    pub items_per_page: u64,
    /// Papers in feed order.
    pub entries: Vec<Paper>,
    /// Set only on a lower-level decode of an Atom error feed; generated `send()` returns `Error::Api`.
    pub problem: Option<Problem>,
}

/// Article metadata. Unknown XML extensions are ignored; links are data and never followed.
#[derive(Clone, Debug)]
pub struct Paper {
    /// Abstract-page URI, including a version when supplied by the provider.
    pub id: String,
    /// Article title, with XML text preserved except surrounding whitespace.
    pub title: String,
    /// Abstract text.
    pub summary: String,
    /// First submission timestamp, retained as provider text.
    pub published: String,
    /// Retrieved version's timestamp, retained as provider text.
    pub updated: String,
    /// Authors in authorship order.
    pub authors: Vec<Author>,
    /// All supplied categories.
    pub categories: Vec<Category>,
    /// Abstract, PDF and DOI links, without fetching them.
    pub links: Vec<Link>,
    /// arXiv primary classification when supplied.
    pub primary_category: Option<Category>,
    /// Optional author comment.
    pub comment: Option<String>,
    /// Optional journal reference.
    pub journal_ref: Option<String>,
    /// Optional DOI.
    pub doi: Option<String>,
}
/// An author and any supplied affiliations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Author {
    /// Author's name.
    pub name: String,
    /// Zero or more arXiv affiliation extensions.
    pub affiliations: Vec<String>,
}
/// An Atom category or arXiv primary category.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Category {
    /// Classification code.
    pub term: String,
    /// Optional classification scheme URI.
    pub scheme: Option<String>,
}
/// A metadata link. The SDK does not resolve or fetch its target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    /// Target as received.
    pub href: String,
    /// Relation, when present.
    pub rel: Option<String>,
    /// Link title, when present.
    pub title: Option<String>,
    /// Media type, when present.
    pub media_type: Option<String>,
}
/// An arXiv Atom error entry. Explicit fields may contain request text.
#[derive(Clone)]
pub struct Problem {
    /// Error identifier URI.
    pub id: String,
    /// Provider's explanation, intentionally absent from error formatting.
    pub summary: String,
}
impl std::fmt::Debug for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Problem(<redacted>)")
    }
}
