//! Places to get books from besides AO3: OPDS catalogs. Project Gutenberg is built in; readers
//! can add others (Calibre's content server, Kavita, Komga, Standard Ebooks for patrons, …).
//! Sites with bot checks aren't sources: their books come in as files (Import).

pub mod dictionary;
pub mod opds;
pub mod xml;

use opds::{Feed, Search};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use ureq::Agent;

/// The largest book downloaded (PDFs can be big).
const MAX_BOOK: u64 = 300 * 1024 * 1024;
const MAX_FEED: u64 = 8 * 1024 * 1024;

/// A catalog: where its feed starts, and a login if it needs one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Catalog {
    pub name: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
}

/// Catalogs that come with the app.
pub fn builtin() -> Vec<Catalog> {
    vec![Catalog { name: "Project Gutenberg".into(), url: "https://www.gutenberg.org/ebooks.opds/".into(), ..Default::default() }]
}

/// What the app tells servers it is. Project Gutenberg asks catalog apps to name themselves with a
/// way to reach their makers.
pub fn user_agent() -> String {
    format!("FlaxReader/{} (+https://github.com/{})", crate::updater::CURRENT_VERSION, crate::updater::GITHUB_REPO)
}

fn host(url: &str) -> &str {
    let rest = url.split("://").nth(1).unwrap_or(url);
    rest.split(['/', '?', '#']).next().unwrap_or(rest)
}

#[derive(Clone)]
pub struct CatalogClient {
    agent: Agent,
}

impl Default for CatalogClient {
    fn default() -> Self {
        Self::new()
    }
}

impl CatalogClient {
    pub fn new() -> Self {
        let agent: Agent = Agent::config_builder()
            .user_agent(user_agent())
            .max_redirects(10)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            .build()
            .into();
        Self { agent }
    }

    /// GETs `url` (at most `limit` bytes). The catalog's login is only ever sent to its own server.
    fn get(&self, url: &str, catalog: &Catalog, limit: u64) -> Result<Vec<u8>, String> {
        let mut req = self.agent.get(url);
        if !catalog.user.is_empty() && host(url) == host(&catalog.url) {
            let token = crate::storage::export::base64(format!("{}:{}", catalog.user, catalog.password).as_bytes());
            req = req.header("Authorization", &format!("Basic {token}"));
        }
        let mut resp = req.call().map_err(|e| format!("Couldn't reach {}: {e}", host(url)))?;
        match resp.status().as_u16() {
            200..=299 => {}
            401 | 403 => return Err(format!("{} wants a user name and password (or refused them).", catalog.name)),
            404 => return Err("That page isn't in the catalog any more.".into()),
            429 => return Err(format!("{} asks to slow down: try again in a minute.", catalog.name)),
            s => return Err(format!("{} answered with an error ({s}).", catalog.name)),
        }
        resp.body_mut().with_config().limit(limit).read_to_vec().map_err(|e| format!("The download stopped: {e}"))
    }

    /// A page of the catalog.
    pub fn feed(&self, url: &str, catalog: &Catalog) -> Result<Feed, String> {
        let bytes = self.get(url, catalog, MAX_FEED)?;
        opds::parse_feed(&String::from_utf8_lossy(&bytes), url)
    }

    /// The results page for `query`, using the search `feed` offers.
    pub fn search(&self, feed: &Feed, query: &str, catalog: &Catalog) -> Result<Feed, String> {
        let template = match &feed.search {
            Some(Search::Template(t)) => t.clone(),
            Some(Search::Description(url)) => {
                let bytes = self.get(url, catalog, MAX_FEED)?;
                opds::parse_opensearch(&String::from_utf8_lossy(&bytes), url).ok_or("This catalog's search isn't one the app can use.")?
            }
            None => return Err("This catalog can't be searched.".into()),
        };
        self.feed(&opds::fill_template(&template, query), catalog)
    }

    /// A book's file.
    pub fn download(&self, url: &str, catalog: &Catalog) -> Result<Vec<u8>, String> {
        self.get(url, catalog, MAX_BOOK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_and_names() {
        assert_eq!(host("https://www.gutenberg.org/ebooks.opds/"), "www.gutenberg.org");
        assert_eq!(host("http://192.168.1.5:8080/opds?x"), "192.168.1.5:8080");
        assert!(user_agent().starts_with("FlaxReader/") && user_agent().contains("github.com"));
    }

    /// Four requests to Project Gutenberg: its start page, search, results and a book's page.
    #[test]
    #[ignore = "reaches Project Gutenberg"]
    fn live_gutenberg() {
        let c = CatalogClient::new();
        let cat = &builtin()[0];
        let root = c.feed(&cat.url, cat).unwrap();
        println!("{}: {} entries, search {:?}", root.title, root.entries.len(), root.search);
        let found = c.search(&root, "pride and prejudice", cat).unwrap();
        let first = &found.entries[0];
        println!("first result: {} ({})", first.title, first.summary);
        let page = c.feed(&first.feed().expect("a book page").href, cat).unwrap();
        let book = page.entries.iter().find_map(|e| e.book()).expect("a download");
        println!("download: {} {}", book.mime, book.href);
        assert!(book.mime.contains("epub"));
    }
}
