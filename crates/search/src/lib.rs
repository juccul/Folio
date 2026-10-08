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
}
pub fn search(conn: &Connection, query: &str) -> rusqlite::Result<Vec<SearchResult>> {
    let tokens: Vec<_> = query
        .split_whitespace()
        .map(|s| format!("\"{}\"*", s.replace('"', "\"\"")))
        .collect();
    if tokens.is_empty() {
        return Ok(vec![]);
    }
    let query = tokens.join(" AND ");
    let mut q=conn.prepare("SELECT note_id,page_id,title,snippet(search,4,'[',']','…',24) FROM search WHERE search MATCH ?1 ORDER BY bm25(search,0,0,3,2,1) LIMIT 100")?;
    let rows = q.query_map(params![query], |r| {
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
            })
        }
    }
    Ok(out)
}
/// Approximate the unicode61 prefix matching used by FTS for object highlighting.
pub fn matches_text(text: &str, query: &str) -> bool {
    use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};
    let fold = |s: &str| {
        s.nfkd()
            .filter(|c| !is_combining_mark(*c))
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    if query.trim().is_empty() {
        return false;
    }
    let text = fold(text);
    let words: Vec<_> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    query.split_whitespace().all(|term| {
        let term = fold(term);
        let parts: Vec<_> = term
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();
        !parts.is_empty()
            && words.windows(parts.len()).any(|window| {
                window
                    .iter()
                    .zip(&parts)
                    .enumerate()
                    .all(|(index, (word, part))| {
                        if index + 1 == parts.len() {
                            word.starts_with(part)
                        } else {
                            word == part
                        }
                    })
            })
    })
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
    fn highlights_follow_unicode_prefix_terms_and_ignore_empty_queries() {
        assert!(matches_text("Café tomorrow", "cafe tom"));
        assert!(matches_text("alpha-beta café", "alpha-be cafe"));
        assert!(!matches_text("Café tomorrow", "cafe yesterday"));
        assert!(!matches_text("anything", ""));
        assert!(!matches_text("anything", "*"));
    }
}
