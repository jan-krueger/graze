pub struct SearchMatcher {
    re: regex::Regex,
}

impl SearchMatcher {
    pub fn new(term: &str) -> Option<Self> {
        let re = regex::RegexBuilder::new(term)
            .case_insensitive(true)
            .build()
            .ok()?;
        Some(SearchMatcher { re })
    }

    pub fn is_match(&self, val: &str) -> bool {
        self.re.is_match(val)
    }
}
