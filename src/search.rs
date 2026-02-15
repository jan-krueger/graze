pub enum SearchMatcher {
    Plain(String),
    Regex(regex::Regex),
}

impl SearchMatcher {
    pub fn new(term: &str, is_regex: bool) -> Option<Self> {
        if is_regex {
            let re = regex::RegexBuilder::new(term)
                .case_insensitive(true)
                .build()
                .ok()?;
            Some(SearchMatcher::Regex(re))
        } else {
            Some(SearchMatcher::Plain(term.to_lowercase()))
        }
    }

    pub fn is_match(&self, val: &str) -> bool {
        match self {
            SearchMatcher::Plain(term) => val.to_lowercase().contains(term.as_str()),
            SearchMatcher::Regex(re) => re.is_match(val),
        }
    }
}
