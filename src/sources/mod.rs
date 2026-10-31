//! Places to get books from besides AO3: OPDS catalogs (Project Gutenberg and Standard Ebooks
//! built in, and any a reader adds: Calibre's content server, Kavita, Komga, …), story forums
//! (SpaceBattles, Sufficient Velocity) and Royal Road's story feeds. Following any of them is
//! `check_feed`.
//!
//! Only sites that answer a plain request are sources. Sites with bot checks (Cloudflare's "Just
//! a moment", say) aren't worked around: their books come in as files (Import).

pub mod accounts;
pub mod dictionary;
pub mod forum;
pub mod opds;
pub mod royalroad;
pub mod xml;

use crate::ao3::client::Ao3Client;
use crate::ao3::models::{Work, WorkMeta};
use crate::ao3::search::{Blurb, Feed as Followed, FeedKind, ItemLink, Listing};
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

const GUTENBERG: &str = "https://www.gutenberg.org/ebooks.opds/";
/// Standard Ebooks' free feed; its full catalogue is for its patrons, who can add it with their
/// sign-in (https://standardebooks.org/feeds/opds).
const STANDARD_EBOOKS: &str = "https://standardebooks.org/feeds/atom/new-releases";

/// Catalogs that come with the app.
pub fn builtin() -> Vec<Catalog> {
    let named = |name: &str, url: String| Catalog { name: name.into(), url, ..Default::default() };
    let mut out = vec![named("Project Gutenberg", GUTENBERG.into()), named("Standard Ebooks", STANDARD_EBOOKS.into())];
    out.extend(forum::SITES.iter().map(|s| named(s.name, forum::root_url(s))));
    out
}

/// Shelves worth following, offered when following a shelf.
pub fn suggested_shelves() -> Vec<Followed> {
    let shelf = |name: &str, link: String, source: &str| Followed { kind: FeedKind::Shelf, name: name.into(), link, source: source.into() };
    let mut out = vec![
        shelf("Latest · Project Gutenberg", "https://www.gutenberg.org/ebooks/search.opds/?sort_order=release_date".into(), GUTENBERG),
        shelf("New releases · Standard Ebooks", STANDARD_EBOOKS.into(), STANDARD_EBOOKS),
    ];
    for s in &forum::SITES {
        let (name, path, _) = s.boards[0];
        out.push(shelf(&format!("{name} · {}", s.name), format!("https://{}/{path}/index.rss", s.host), &forum::root_url(s)));
    }
    out
}

/// A date from a feed (RFC 3339 or RFC 2822) as "09 Mar 2024", the way AO3 shows them.
pub(crate) fn day(s: &str) -> String {
    let s = s.trim();
    chrono::DateTime::parse_from_rfc3339(s)
        .or_else(|_| chrono::DateTime::parse_from_rfc2822(s))
        .map(|d| d.format("%d %b %Y").to_string())
        .unwrap_or_default()
}

/// What the app tells servers it is. Project Gutenberg asks catalog apps to name themselves with a
/// way to reach their makers.
pub fn user_agent() -> String {
    format!("FlaxReader/{} (+https://github.com/{})", crate::updater::CURRENT_VERSION, crate::updater::GITHUB_REPO)
}

/// Whether a refused page is a bot check (Cloudflare's "Just a moment" page) rather than the
/// site's own answer. Ordinary pages carry Cloudflare's background script too
/// ("challenge-platform"), so that alone doesn't count.
pub(crate) fn is_bot_check(body: &str) -> bool {
    body.contains("Just a moment") || body.contains("_cf_chl_opt") || body.contains("cf-chl-")
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
            403 | 503 => {
                // A bot check isn't something to get around: say so plainly.
                let body = resp.body_mut().with_config().limit(256 * 1024).read_to_string().unwrap_or_default();
                if is_bot_check(&body) {
                    return Err(format!("{} is asking for a browser check, which Flax Reader doesn't do. Open it in your browser instead.", catalog.name));
                }
                return Err(format!("{} wants a user name and password (or refused them).", catalog.name));
            }
            401 => return Err(format!("{} wants a user name and password (or refused them).", catalog.name)),
            404 => return Err("That page isn't in the catalog any more.".into()),
            429 => return Err(format!("{} asks to slow down: try again in a minute.", catalog.name)),
            s => return Err(format!("{} answered with an error ({s}).", catalog.name)),
        }
        resp.body_mut().with_config().limit(limit).read_to_vec().map_err(|e| format!("The download stopped: {e}"))
    }

    /// A page from a site that isn't a catalog the reader set up (a forum, Royal Road), as text.
    pub(crate) fn page(&self, url: &str) -> Result<String, String> {
        let name = forum::site_of(url).map_or_else(|| if royalroad::fiction_id(url).is_some() { "Royal Road".to_string() } else { host(url).to_string() }, |s| s.name.to_string());
        let site = Catalog { name, url: url.into(), ..Default::default() };
        Ok(String::from_utf8_lossy(&self.get(url, &site, MAX_FEED)?).into_owned())
    }

    /// A page of the catalog.
    pub fn feed(&self, url: &str, catalog: &Catalog) -> Result<Feed, String> {
        if forum::site_of(url).is_some() {
            return forum::feed(self, url);
        }
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

    /// A book or story, ready for the library: a forum story's chapters, or a book's file opened.
    /// An Atom `mime` means `url` is the book's own page, whose best copy is downloaded.
    pub fn get_work(&self, url: &str, mime: &str, catalog: &Catalog, title: Option<&str>) -> Result<Work, String> {
        if mime == opds::STORY_MIME || forum::thread_url(url).is_some() {
            return forum::download(self, url);
        }
        let href = if mime.starts_with("application/atom+xml") {
            let page = self.feed(url, catalog)?;
            page.entries.iter().find_map(|e| e.book()).map(|l| l.href.clone()).ok_or("There's no EPUB or PDF of this book to download.")?
        } else {
            url.to_string()
        };
        crate::ao3::epub::import_named(&self.download(&href, catalog)?, title)
    }
}

/// The catalog a shelf belongs to: one the reader set up (with its sign-in), or a built-in one.
fn catalog_for(source: &str, catalogs: &[Catalog]) -> Catalog {
    builtin()
        .into_iter()
        .chain(catalogs.iter().cloned())
        .find(|c| c.url == source)
        .unwrap_or_else(|| Catalog { name: host(source).to_string(), url: source.to_string(), ..Default::default() })
}

/// A catalogue's entry as a followed item.
fn entry_blurb(e: &opds::Entry, catalog: &str) -> Blurb {
    let mut meta = WorkMeta { id: format!("opds:{}", if e.id.is_empty() { &e.title } else { &e.id }), title: e.title.clone(), authors: e.authors.clone(), ..WorkMeta::default() };
    meta.fandoms = Vec::new();
    meta.summary_text = e.summary.clone();
    meta.language = e.language.clone();
    let link = e.book().or_else(|| e.feed()).map(|l| ItemLink::Book { href: l.href.clone(), mime: l.mime.clone(), catalog: catalog.to_string() });
    Blurb { meta, updated: e.updated.clone(), version: String::new(), link }
}

/// What a followed feed holds now. A followed forum story saved in the library (`saved`) comes
/// back with its new chapters added.
pub fn check_feed(feed: &Followed, saved: Option<Work>, ao3: &Ao3Client, client: &CatalogClient, catalogs: &[Catalog]) -> Result<(Listing, Option<Work>), String> {
    let one = |b: Blurb| Listing { works: vec![b], total: None, page: 1, has_next: false };
    match feed.kind {
        FeedKind::Story if forum::thread_url(&feed.link).is_some() => {
            let (b, work) = forum::check(client, &feed.link, saved)?;
            Ok((one(b), work))
        }
        FeedKind::Story => Ok((one(royalroad::check(client, &feed.link)?), None)),
        FeedKind::Shelf => {
            let catalog = catalog_for(&feed.source, catalogs);
            let page = client.feed(&feed.link, &catalog)?;
            let works = page.entries.iter().map(|e| entry_blurb(e, &catalog.url)).collect();
            Ok((Listing { works, total: None, page: 1, has_next: false }, None))
        }
        _ => Ok((ao3.feed(feed, 1)?, None)),
    }
}

/// The library id of a followed story kept in the library (forum stories).
pub fn story_work_id(feed: &Followed) -> Option<String> {
    (feed.kind == FeedKind::Story).then(|| forum::story_id(&feed.link)).flatten()
}

/// Starts following a story from a link: a forum story is downloaded whole (and comes back to
/// be saved); a Royal Road story is looked up. Returns the feed, its first listing (the baseline
/// that later checks compare with) and the story to save.
pub fn follow_story(url: &str, client: &CatalogClient) -> Result<(Followed, Listing, Option<Work>), String> {
    let url = url.trim();
    let listing = |b: Blurb| Listing { works: vec![b], total: None, page: 1, has_next: false };
    if let Some(thread) = forum::thread_url(url) {
        let work = forum::download(client, &thread)?;
        let feed = Followed { kind: FeedKind::Story, name: work.meta.title.clone(), link: thread.clone(), source: String::new() };
        let b = forum::blurb_of(&work, &thread);
        return Ok((feed, listing(b), Some(work)));
    }
    if let Some(id) = royalroad::fiction_id(url) {
        let link = royalroad::story_url(id);
        let b = royalroad::check(client, &link)?;
        let feed = Followed { kind: FeedKind::Story, name: b.meta.title.clone(), link, source: String::new() };
        return Ok((feed, listing(b), None));
    }
    if url.contains("archiveofourown.org/works/") {
        return Err("New chapters of an AO3 work show up when you follow its author.".into());
    }
    Err("Paste a link to a story on SpaceBattles, Sufficient Velocity or Royal Road.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_dates() {
        assert_eq!(day("2026-10-27T21:16:59Z"), "27 Oct 2026");
        assert_eq!(day("Sat, 31 Oct 2026 03:34:57 +0000"), "31 Oct 2026");
        assert_eq!(day("Thu, 06 Jul 2023 17:47:41 GMT"), "06 Jul 2023");
        assert_eq!(day("soon"), "");
    }

    #[test]
    fn standard_ebooks_feed() {
        let atom = r#"<?xml version="1.0"?><feed xmlns="http://www.w3.org/2005/Atom"><title>Standard Ebooks - Newest Ebooks</title>
<entry><id>https://standardebooks.org/ebooks/joseph-conrad/short-fiction</id><title>Short Fiction</title><author><name>Joseph Conrad</name></author>
<updated>2026-10-27T21:16:59Z</updated><summary type="text">The complete short fiction of Joseph Conrad.</summary>
<link href="https://standardebooks.org/ebooks/joseph-conrad/short-fiction" rel="alternate" type="application/xhtml+xml"/>
<link href="https://standardebooks.org/x.epub" rel="enclosure" title="Recommended compatible epub" type="application/epub+zip"/>
<link href="https://standardebooks.org/x_advanced.epub" rel="enclosure" title="Advanced epub" type="application/epub+zip"/>
<link href="https://standardebooks.org/x.kepub.epub" rel="enclosure" type="application/kepub+zip"/></entry></feed>"#;
        let f = opds::parse_feed(atom, STANDARD_EBOOKS).unwrap();
        let e = &f.entries[0];
        assert_eq!(e.book().unwrap().href, "https://standardebooks.org/x.epub");
        assert_eq!(e.updated, "27 Oct 2026");
        let b = entry_blurb(e, STANDARD_EBOOKS);
        assert_eq!((b.meta.id.as_str(), b.meta.authors[0].as_str()), ("opds:https://standardebooks.org/ebooks/joseph-conrad/short-fiction", "Joseph Conrad"));
        assert!(matches!(b.link, Some(ItemLink::Book { ref mime, .. }) if mime == "application/epub+zip"));
        assert_eq!(builtin().len(), 4);
        assert!(follow_story("https://example.com/story", &CatalogClient::new()).is_err());
    }

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
