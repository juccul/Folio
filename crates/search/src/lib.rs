//! Local Unicode-aware FTS5 search. Queries are quoted to prevent syntax errors or
//! operators entered by users from becoming a different query.
use folio_document::*;
use rusqlite::{Connection, params};
#[derive(Clone, Debug)]
pub struct SearchResult {
    pub note: Id,
    pub page: Id,
    pub title: String,
    pub snippet: String,
    pub page_number: Option<usize>,
}
pub fn search(conn: &Connection, query: &str) -> rusqlite::Result<Vec<SearchResult>> {
    let mut expression = String::with_capacity(query.len());
    for (index, token) in query.split_whitespace().enumerate() {
        if index > 0 {
            expression.push_str(" AND ");
        }
        expression.push('"');
        for character in token.chars() {
            expression.push(character);
            if character == '"' {
                expression.push('"');
            }
        }
        expression.push_str("\"*");
    }
    if expression.is_empty() {
        return Ok(vec![]);
    }
    let mut q=conn.prepare_cached("SELECT note_id,page_id,title,snippet(search,4,'[',']','…',24) FROM search WHERE search MATCH ?1 ORDER BY bm25(search,0,0,3,2,1) LIMIT 100")?;
    let rows = q.query_map(params![expression], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    let mut out = vec![];
    for row in rows {
        let (n, p, title, snippet) = row?;
        if let (Ok(note), Ok(page)) = (Id::parse_str(&n), Id::parse_str(&p)) {
            out.push(SearchResult {
                note,
                page,
                title,
                snippet,
                page_number: None,
            })
        }
    }
    Ok(out)
}
/// Pre-normalized query reused while highlighting many objects or annotations.
#[derive(Clone, Debug)]
pub struct TextMatcher {
    terms: Vec<Vec<String>>,
}
fn fold(text: &str) -> String {
    use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};
    text.nfkd()
        .filter(|character| !is_combining_mark(*character))
        .flat_map(char::to_lowercase)
        .collect()
}
impl TextMatcher {
    pub fn new(query: &str) -> Self {
        Self {
            terms: query
                .split_whitespace()
                .map(|term| {
                    fold(term)
                        .split(|character: char| !character.is_alphanumeric())
                        .filter(|part| !part.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .collect(),
        }
    }
    /// Approximate unicode61 phrase-prefix matching with diacritic folding.
    pub fn matches(&self, text: &str) -> bool {
        if self.terms.is_empty() || self.terms.iter().any(Vec::is_empty) {
            return false;
        }
        let text = fold(text);
        let words: Vec<_> = text
            .split(|character: char| !character.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .collect();
        self.terms.iter().all(|parts| {
            words.windows(parts.len()).any(|window| {
                window
                    .iter()
                    .zip(parts)
                    .enumerate()
                    .all(|(index, (word, part))| {
                        if index + 1 == parts.len() {
                            word.starts_with(part)
                        } else {
                            *word == part
                        }
                    })
            })
        })
    }
}
/// Approximate the unicode61 prefix matching used by FTS for object highlighting.
pub fn matches_text(text: &str, query: &str) -> bool {
    TextMatcher::new(query).matches(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_prefix_and_safe_queries() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE VIRTUAL TABLE search USING fts5(note_id UNINDEXED,page_id UNINDEXED,title,tags,body,tokenize='unicode61 remove_diacritics 2')").unwrap();
        let n = Id::new_v4();
        let p = Id::new_v4();
        c.execute(
            "INSERT INTO search VALUES(?1,?2,'Café','ideas','handwritten café tomorrow')",
            params![n.to_string(), p.to_string()],
        )
        .unwrap();
        assert_eq!(search(&c, "cafe tom").unwrap().len(), 1);
        assert_eq!(search(&c, "hand").unwrap()[0].page, p);
        assert!(search(&c, "\" OR *").is_ok());
        assert!(search(&c, "").unwrap().is_empty());
    }
    #[test]
    fn reusable_matcher_preserves_phrase_prefix_and_unicode_semantics() {
        let matcher = TextMatcher::new("ALPHA-be café");
        assert!(matcher.matches("Alpha-beta CAFE"));
        assert!(matcher.matches("alpha-berry caffè café"));
        assert!(!matcher.matches("alphabet-beta café"));
        assert!(!matcher.matches("alpha-be elsewhere"));
        for query in ["", "  ", "*", "cafe *"] {
            assert!(!TextMatcher::new(query).matches("Café tomorrow"));
        }
    }

    #[test]
    fn highlights_follow_unicode_prefix_terms_and_ignore_empty_queries() {
        assert!(matches_text("Café tomorrow", "cafe tom"));
        assert!(matches_text("alpha-beta café", "alpha-be cafe"));
        assert!(!matches_text("Café tomorrow", "cafe yesterday"));
        assert!(!matches_text("anything", ""));
        assert!(!matches_text("anything", "*"));
    }
}
