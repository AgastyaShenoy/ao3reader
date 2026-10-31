//! AO3 work search and work listings (search results, tag, author and collection feeds).
//!
//! All of these pages list works as the same "blurb" (`li.work.blurb`), so one parser
//! covers every feed. Written from AO3's public markup; AO3 was down for maintenance while
//! this was built, so it is covered by fixtures rather than live pages.

use crate::ao3::models::{Category, WorkMeta, WorkRating, WorkStats};
use crate::ao3::parser::Ao3Parser;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};

pub const AO3: &str = "https://archiveofourown.org";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    #[default]
    BestMatch,
    Author,
    Title,
    Posted,
    Updated,
    Words,
    Hits,
    Kudos,
    Comments,
    Bookmarks,
}

impl SortBy {
    pub const ALL: [SortBy; 10] = [
        SortBy::BestMatch,
        SortBy::Updated,
        SortBy::Posted,
        SortBy::Kudos,
        SortBy::Hits,
        SortBy::Bookmarks,
        SortBy::Comments,
        SortBy::Words,
        SortBy::Title,
        SortBy::Author,
    ];

    fn column(self) -> &'static str {
        match self {
            SortBy::BestMatch => "_score",
            SortBy::Author => "authors_to_sort_on",
            SortBy::Title => "title_to_sort_on",
            SortBy::Posted => "created_at",
            SortBy::Updated => "revised_at",
            SortBy::Words => "word_count",
            SortBy::Hits => "hits",
            SortBy::Kudos => "kudos_count",
            SortBy::Comments => "comments_count",
            SortBy::Bookmarks => "bookmarks_count",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortBy::BestMatch => "Best match",
            SortBy::Author => "Author",
            SortBy::Title => "Title",
            SortBy::Posted => "Date posted",
            SortBy::Updated => "Date updated",
            SortBy::Words => "Word count",
            SortBy::Hits => "Hits",
            SortBy::Kudos => "Kudos",
            SortBy::Comments => "Comments",
            SortBy::Bookmarks => "Bookmarks",
        }
    }
}

/// "Any", or only one side of a yes/no filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Tri {
    #[default]
    Any,
    Only,
    Exclude,
}

/// Everything AO3's work search form offers that is worth having on a phone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SearchQuery {
    /// Searches every field, like the box on AO3's search page.
    pub query: String,
    pub title: String,
    pub creators: String,
    /// Comma-separated tag names.
    pub fandoms: String,
    pub relationships: String,
    pub characters: String,
    pub tags: String,
    pub rating: Option<WorkRating>,
    /// Any of these categories (F/F, F/M, Gen, M/M, Multi, Other).
    pub categories: Vec<String>,
    /// Only complete works (`Only`) or only works in progress (`Exclude`).
    pub complete: Tri,
    pub crossovers: Tri,
    pub single_chapter: bool,
    pub words_min: Option<u32>,
    pub words_max: Option<u32>,
    /// AO3 language code such as "en"; empty for any.
    pub language: String,
    pub sort: SortBy,
    /// Ascending instead of descending.
    pub ascending: bool,
}

fn rating_id(r: WorkRating) -> &'static str {
    match r {
        WorkRating::NotRated => "9",
        WorkRating::General => "10",
        WorkRating::Teen => "11",
        WorkRating::Mature => "12",
        WorkRating::Explicit => "13",
    }
}

fn category_id(c: &str) -> Option<&'static str> {
    Some(match c {
        "F/F" => "116",
        "F/M" => "22",
        "Gen" => "21",
        "M/M" => "23",
        "Multi" => "2246",
        "Other" => "24",
        _ => return None,
    })
}

/// Percent-encodes a query value (form encoding, spaces as `+`).
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Percent-encodes a URL path segment (spaces as `%20`).
fn encode_path(s: &str) -> String {
    encode(s).replace('+', "%20")
}

/// AO3 spells some characters in tag URLs with its own escapes (`/` is `*s*`, `.` is `*d*`).
pub fn tag_path(tag: &str) -> String {
    let escaped = tag.trim().replace('/', "*s*").replace('&', "*a*").replace('.', "*d*").replace('?', "*q*").replace('#', "*h*");
    // Percent-encode the rest, keeping AO3's `*` markers as they are.
    encode_path(&escaped).replace("%2A", "*")
}

impl SearchQuery {
    pub fn is_empty(&self) -> bool {
        let blank = |s: &String| s.trim().is_empty();
        blank(&self.query)
            && blank(&self.title)
            && blank(&self.creators)
            && blank(&self.fandoms)
            && blank(&self.relationships)
            && blank(&self.characters)
            && blank(&self.tags)
            && self.rating.is_none()
            && self.categories.is_empty()
            && self.complete == Tri::Any
            && self.crossovers == Tri::Any
            && !self.single_chapter
            && self.words_min.is_none()
            && self.words_max.is_none()
            && blank(&self.language)
    }

    /// The `/works/search` URL for a page of results (1-based).
    pub fn url(&self, page: u32) -> String {
        let mut p: Vec<(String, String)> = Vec::new();
        let mut field = |k: &str, v: &str| p.push((format!("work_search[{k}]"), v.trim().to_string()));
        field("query", &self.query);
        field("title", &self.title);
        field("creators", &self.creators);
        field("fandom_names", &self.fandoms);
        field("relationship_names", &self.relationships);
        field("character_names", &self.characters);
        field("freeform_names", &self.tags);
        field("rating_ids", self.rating.map(rating_id).unwrap_or(""));
        field(
            "complete",
            match self.complete {
                Tri::Any => "",
                Tri::Only => "T",
                Tri::Exclude => "F",
            },
        );
        field(
            "crossover",
            match self.crossovers {
                Tri::Any => "",
                Tri::Only => "T",
                Tri::Exclude => "F",
            },
        );
        field("single_chapter", if self.single_chapter { "1" } else { "0" });
        let words = match (self.words_min, self.words_max) {
            (Some(a), Some(b)) => format!("{a}-{b}"),
            (Some(a), None) => format!(">{a}"),
            (None, Some(b)) => format!("<{b}"),
            (None, None) => String::new(),
        };
        field("word_count", &words);
        field("language_id", &self.language);
        field("sort_column", self.sort.column());
        field("sort_direction", if self.ascending { "asc" } else { "desc" });
        for c in &self.categories {
            if let Some(id) = category_id(c) {
                p.push(("work_search[category_ids][]".into(), id.into()));
            }
        }
        p.push(("commit".into(), "Search".into()));
        if page > 1 {
            p.push(("page".into(), page.to_string()));
        }
        let qs: Vec<String> = p.iter().map(|(k, v)| format!("{}={}", encode(k), encode(v))).collect();
        format!("{AO3}/works/search?{}", qs.join("&"))
    }
}

/// What a followed feed is: an AO3 author, tag or collection, one story (on a forum or Royal
/// Road), or a catalogue's shelf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedKind {
    Author,
    Tag,
    Collection,
    Story,
    Shelf,
}

impl FeedKind {
    pub fn label(self) -> &'static str {
        match self {
            FeedKind::Author => "Author",
            FeedKind::Tag => "Tag",
            FeedKind::Collection => "Collection",
            FeedKind::Story => "Story",
            FeedKind::Shelf => "Shelf",
        }
    }

    /// A listing on AO3 (as opposed to a story or a catalogue's shelf).
    pub fn is_ao3(self) -> bool {
        matches!(self, FeedKind::Author | FeedKind::Tag | FeedKind::Collection)
    }
}

/// Something followed: a work listing on AO3 (an author's, a tag's or a collection's works), a
/// story, or a catalogue's shelf.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Feed {
    pub kind: FeedKind,
    /// User name (optionally `user/pseud`), tag name, or collection name; a story's or shelf's
    /// title.
    pub name: String,
    /// A story's or shelf's address.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link: String,
    /// The catalogue a shelf is in (its start address).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

impl Feed {
    /// An AO3 author, tag or collection.
    pub fn new(kind: FeedKind, name: impl Into<String>) -> Feed {
        Feed { kind, name: name.into(), link: String::new(), source: String::new() }
    }

    /// Reads an AO3 link (`/users/x`, `/tags/x/works`, `/collections/x`) or a plain name of `kind`.
    pub fn parse(input: &str, kind: FeedKind) -> Option<Feed> {
        let s = input.trim();
        if s.is_empty() {
            return None;
        }
        if s.contains("archiveofourown.org/") || s.starts_with('/') {
            let path = s.split("archiveofourown.org").last().unwrap_or(s);
            let path = path.split(['?', '#']).next().unwrap_or("");
            let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
            let decode = |p: &str| unescape_tag(&percent_decode(p));
            return match parts.as_slice() {
                ["users", user, "pseuds", pseud, ..] => {
                    let (user, pseud) = (percent_decode(user), percent_decode(pseud));
                    // A user's default pseud is the user.
                    Some(Feed::new(FeedKind::Author, if user.eq_ignore_ascii_case(&pseud) { user } else { format!("{user}/{pseud}") }))
                }
                ["users", user, ..] => Some(Feed::new(FeedKind::Author, percent_decode(user))),
                ["tags", tag, ..] => Some(Feed::new(FeedKind::Tag, decode(tag))),
                ["collections", c, ..] => Some(Feed::new(FeedKind::Collection, percent_decode(c))),
                _ => None,
            };
        }
        Some(Feed::new(kind, s))
    }

    pub fn id(&self) -> String {
        let key = if self.link.is_empty() { &self.name } else { &self.link };
        format!("{}:{key}", self.kind.label().to_lowercase())
    }

    /// Newest-first listing URL for a page (1-based); a story's or shelf's own address.
    pub fn url(&self, page: u32) -> String {
        if !self.kind.is_ao3() {
            return self.link.clone();
        }
        let base = match self.kind {
            FeedKind::Author => match self.name.split_once('/') {
                Some((user, pseud)) => format!("{AO3}/users/{}/pseuds/{}/works", encode_path(user), encode_path(pseud)),
                None => format!("{AO3}/users/{}/works", encode_path(&self.name)),
            },
            FeedKind::Tag => format!("{AO3}/tags/{}/works", tag_path(&self.name)),
            FeedKind::Collection => format!("{AO3}/collections/{}/works", encode_path(&self.name)),
            FeedKind::Story | FeedKind::Shelf => unreachable!(),
        };
        if page > 1 {
            format!("{base}?page={page}")
        } else {
            base
        }
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn unescape_tag(s: &str) -> String {
    s.replace("*s*", "/").replace("*a*", "&").replace("*d*", ".").replace("*q*", "?").replace("*h*", "#")
}

/// One work in a listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Blurb {
    pub meta: WorkMeta,
    /// Date shown on the blurb: when the work was last updated, e.g. "09 Mar 2024".
    pub updated: String,
    /// What changes when the work does, for following; AO3 works use their date. A story's is its
    /// newest chapter.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    /// Where it leads, for what isn't an AO3 work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<ItemLink>,
}

/// What a followed item that isn't an AO3 work leads to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemLink {
    /// A book in a catalogue (`catalog` is the catalogue's start address). An Atom `mime` means
    /// the book's own page, whose best copy gets downloaded.
    Book { href: String, mime: String, catalog: String },
    /// A forum story, kept in the library (its id is the item's) and brought up to date.
    Story { url: String },
    /// A page to read in the browser.
    Web { url: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Listing {
    pub works: Vec<Blurb>,
    /// Total number of works, when AO3 states it.
    pub total: Option<u64>,
    pub page: u32,
    pub has_next: bool,
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("valid selector")
}

fn text(e: ElementRef) -> String {
    let flat: String = e.text().flat_map(str::chars).map(|c| if c.is_whitespace() { ' ' } else { c }).collect();
    Ao3Parser::collapse_whitespace(&flat).trim().to_string()
}

fn number(s: &str) -> u64 {
    s.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)
}

fn parse_blurb(li: ElementRef) -> Option<Blurb> {
    let mut m = WorkMeta::default();
    let id = li.value().id().and_then(|i| i.strip_prefix("work_")).map(str::to_string).or_else(|| {
        li.select(&sel("h4.heading a[href^='/works/']")).next()?.value().attr("href")?.trim_start_matches("/works/").split('/').next().map(str::to_string)
    })?;
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    m.id = id;
    let heading = li.select(&sel("h4.heading")).next()?;
    m.title = heading.select(&sel("a[href^='/works/']")).next().map(text).unwrap_or_else(|| "Untitled".into());
    let authors: Vec<String> = heading.select(&sel("a[rel='author']")).map(text).collect();
    m.authors = if authors.is_empty() { vec!["Anonymous".into()] } else { authors };
    m.is_restricted = heading.select(&sel("img[title='Restricted'], img[alt*='Restricted']")).next().is_some();
    m.fandoms = li.select(&sel("h5.fandoms a.tag")).map(text).collect();

    let title_of = |css: &str| li.select(&sel(css)).next().and_then(|e| e.value().attr("title")).unwrap_or("").to_string();
    m.rating = WorkRating::from_str_loose(&title_of("ul.required-tags span.rating"));
    m.categories = title_of("ul.required-tags span.category")
        .split(',')
        .map(str::trim)
        .filter(|c| !c.is_empty() && !c.eq_ignore_ascii_case("No category"))
        .map(Category::from_str_loose)
        .collect();
    m.is_complete = title_of("ul.required-tags span.iswip").eq_ignore_ascii_case("Complete Work");

    let tags = |class: &str| li.select(&sel(&format!("ul.tags li.{class} a.tag"))).map(text).collect::<Vec<_>>();
    m.warnings = tags("warnings");
    if m.warnings.is_empty() {
        let w = title_of("ul.required-tags span.warnings");
        m.warnings = w.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
    }
    m.relationships = tags("relationships");
    m.characters = tags("characters");
    m.freeform_tags = tags("freeforms");

    if let Some(s) = li.select(&sel("blockquote.summary")).next() {
        m.summary_text = Ao3Parser::extract_paragraphs(s).join("\n\n");
        if m.summary_text.is_empty() {
            m.summary_text = text(s);
        }
    }

    let dd = |class: &str| li.select(&sel(&format!("dl.stats dd.{class}"))).next().map(text).unwrap_or_default();
    let lang = dd("language");
    if !lang.is_empty() {
        m.language = lang;
    }
    let chapters = dd("chapters");
    let (published, total) = chapters.split_once('/').unwrap_or((chapters.as_str(), "?"));
    m.stats = WorkStats {
        words: number(&dd("words")) as usize,
        chapters_published: number(published).max(1) as u32,
        chapters_total: total.trim().parse().ok(),
        comments: number(&dd("comments")),
        kudos: number(&dd("kudos")),
        bookmarks: number(&dd("bookmarks")),
        hits: number(&dd("hits")),
        ..WorkStats::default()
    };
    let updated = li.select(&sel("p.datetime")).next().map(text).unwrap_or_default();
    m.stats.published_date = updated.clone();
    Some(Blurb { meta: m, updated, ..Default::default() })
}

/// Why an AO3 page that should list works can't, in reader terms.
pub fn listing_problem(html: &str) -> Option<String> {
    if html.contains("can't be filtered on") || html.contains("has not been marked common") {
        return Some("AO3 doesn't list works for this tag on its own. Use AO3's main tag for it: open the tag on AO3 and copy the link of the tag it points to.".into());
    }
    None
}

/// Parses any AO3 page that lists works as blurbs.
pub fn parse_listing(html: &str, page: u32) -> Listing {
    let doc = Html::parse_document(html);
    let works: Vec<Blurb> = doc.select(&sel("li.work.blurb")).filter_map(parse_blurb).collect();
    let count = Regex::new(r"(?i)of\s+([\d,]+)\s+works?\b|([\d,]+)\s+works?\s+found").expect("regex");
    let total = doc.select(&sel("h2.heading, h3.heading")).find_map(|h| {
        let t = text(h);
        count.captures(&t).and_then(|c| c.get(1).or_else(|| c.get(2))).map(|m| number(m.as_str()))
    });
    let total = total.or_else(|| if works.is_empty() { Some(0) } else { None });
    let has_next = doc.select(&sel("ol.pagination li.next a[href]")).next().is_some();
    Listing { works, total, page: page.max(1), has_next }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r##"<html><body><div id="main">
<h3 class="heading">1 - 2 of 1,234 Works found</h3>
<ol class="work index group">
<li id="work_111" class="work blurb group" role="article">
  <div class="header module">
    <h4 class="heading"><a href="/works/111">Golden Hour</a> by <a rel="author" href="/users/ana/pseuds/ana">ana</a>, <a rel="author" href="/users/bo/pseuds/bo">bo</a>
      <img alt="(Restricted)" title="Restricted" src="/images/lockblue.png"></h4>
    <h5 class="fandoms heading"><span class="landmark">Fandoms:</span> <a class="tag" href="/tags/Harry%20Potter/works">Harry Potter - J. K. Rowling</a></h5>
    <ul class="required-tags">
      <li><a><span class="rating-teen rating" title="Teen And Up Audiences"><span class="text">Teen</span></span></a></li>
      <li><a><span class="warning-no warnings" title="No Archive Warnings Apply"><span class="text">No warnings</span></span></a></li>
      <li><a><span class="category-het category" title="F/M, Gen"><span class="text">F/M</span></span></a></li>
      <li><a><span class="complete-no iswip" title="Work in Progress"><span class="text">WIP</span></span></a></li>
    </ul>
    <p class="datetime">09 Mar 2024</p>
  </div>
  <ul class="tags commas">
    <li class="warnings"><strong><a class="tag" href="#">No Archive Warnings Apply</a></strong></li>
    <li class="relationships"><a class="tag" href="#">Hermione Granger/Harry Potter</a></li>
    <li class="characters"><a class="tag" href="#">Harry Potter</a></li>
    <li class="freeforms"><a class="tag" href="#">Fluff</a></li>
  </ul>
  <blockquote class="userstuff summary"><p>They <em>finally</em> talk.</p><p>Second line.</p></blockquote>
  <dl class="stats">
    <dt class="language">Language:</dt><dd class="language" lang="en">English</dd>
    <dt class="words">Words:</dt><dd class="words">12,345</dd>
    <dt class="chapters">Chapters:</dt><dd class="chapters"><a href="/works/111/chapters/9">3</a>/?</dd>
    <dt class="comments">Comments:</dt><dd class="comments"><a href="#">45</a></dd>
    <dt class="kudos">Kudos:</dt><dd class="kudos"><a href="#">1,678</a></dd>
    <dt class="bookmarks">Bookmarks:</dt><dd class="bookmarks"><a href="#">90</a></dd>
    <dt class="hits">Hits:</dt><dd class="hits">23,456</dd>
  </dl>
</li>
<li id="work_222" class="work blurb group" role="article">
  <div class="header module">
    <h4 class="heading"><a href="/works/222">Untold</a> by Anonymous</h4>
    <ul class="required-tags">
      <li><a><span class="rating-explicit rating" title="Explicit"></span></a></li>
      <li><a><span class="complete-yes iswip" title="Complete Work"></span></a></li>
    </ul>
    <p class="datetime">01 Jan 2020</p>
  </div>
  <dl class="stats"><dd class="words">900</dd><dd class="chapters">1/1</dd></dl>
</li>
</ol>
<ol class="pagination actions"><li class="previous"><span class="disabled">Previous</span></li><li class="next"><a rel="next" href="/works/search?page=2">Next</a></li></ol>
</div></body></html>"##;

    #[test]
    fn blurbs_and_paging() {
        let l = parse_listing(PAGE, 1);
        assert_eq!(l.total, Some(1234));
        assert!(l.has_next);
        assert_eq!(l.works.len(), 2);
        let w = &l.works[0].meta;
        assert_eq!(w.id, "111");
        assert_eq!(w.title, "Golden Hour");
        assert_eq!(w.authors, vec!["ana", "bo"]);
        assert!(w.is_restricted);
        assert_eq!(w.fandoms, vec!["Harry Potter - J. K. Rowling"]);
        assert_eq!(w.rating, WorkRating::Teen);
        assert_eq!(w.categories, vec![Category::FM, Category::Gen]);
        assert_eq!(w.warnings, vec!["No Archive Warnings Apply"]);
        assert_eq!(w.relationships, vec!["Hermione Granger/Harry Potter"]);
        assert_eq!(w.freeform_tags, vec!["Fluff"]);
        assert_eq!(w.summary_text, "They *finally* talk.\n\nSecond line.");
        assert_eq!((w.stats.words, w.stats.chapters_published, w.stats.chapters_total), (12345, 3, None));
        assert_eq!((w.stats.kudos, w.stats.hits, w.stats.comments, w.stats.bookmarks), (1678, 23456, 45, 90));
        assert!(!w.is_complete);
        assert_eq!(l.works[0].updated, "09 Mar 2024");
        let anon = &l.works[1].meta;
        assert_eq!(anon.authors, vec!["Anonymous"]);
        assert!(anon.is_complete);
        assert_eq!(anon.stats.chapters_total, Some(1));
    }

    #[test]
    fn empty_results() {
        let l = parse_listing("<h3 class=\"heading\">0 Found</h3><p>No results found.</p>", 1);
        assert!(l.works.is_empty());
        assert_eq!(l.total, Some(0));
        assert!(!l.has_next);
    }

    #[test]
    fn search_url() {
        let q = SearchQuery {
            query: "time travel".into(),
            relationships: "Hermione Granger/Harry Potter".into(),
            rating: Some(WorkRating::Teen),
            categories: vec!["F/M".into()],
            complete: Tri::Only,
            words_min: Some(10000),
            sort: SortBy::Kudos,
            ..Default::default()
        };
        let u = q.url(2);
        assert!(u.starts_with("https://archiveofourown.org/works/search?"));
        assert!(u.contains("work_search%5Bquery%5D=time+travel"));
        assert!(u.contains("work_search%5Brelationship_names%5D=Hermione+Granger%2FHarry+Potter"));
        assert!(u.contains("work_search%5Brating_ids%5D=11"));
        assert!(u.contains("work_search%5Bcategory_ids%5D%5B%5D=22"));
        assert!(u.contains("work_search%5Bcomplete%5D=T"));
        assert!(u.contains("work_search%5Bword_count%5D=%3E10000"));
        assert!(u.contains("work_search%5Bsort_column%5D=kudos_count"));
        assert!(u.ends_with("&page=2"));
    }

    #[test]
    fn feeds() {
        let tag = Feed::new(FeedKind::Tag, "Hermione Granger/Harry Potter");
        assert_eq!(tag.url(1), "https://archiveofourown.org/tags/Hermione%20Granger*s*Harry%20Potter/works");
        let dotted = Feed::new(FeedKind::Tag, "Harry Potter - J. K. Rowling");
        assert_eq!(dotted.url(2), "https://archiveofourown.org/tags/Harry%20Potter%20-%20J*d*%20K*d*%20Rowling/works?page=2");
        let parsed = Feed::parse("https://archiveofourown.org/tags/Hermione%20Granger*s*Harry%20Potter/works?page=3", FeedKind::Author).unwrap();
        assert_eq!(parsed, tag);
        assert_eq!(Feed::parse("archiveofourown.org/users/ana/pseuds/Ana2/works", FeedKind::Tag).unwrap().name, "ana/Ana2");
        assert_eq!(Feed::parse("https://archiveofourown.org/collections/hp_harmony", FeedKind::Tag).unwrap(), Feed::new(FeedKind::Collection, "hp_harmony"));
        assert_eq!(Feed::parse("someone", FeedKind::Author).unwrap().url(1), "https://archiveofourown.org/users/someone/works");
        assert_eq!(Feed::parse("https://archiveofourown.org/users/ana/pseuds/ana/works", FeedKind::Tag).unwrap().name, "ana");
        let plus = Feed::parse("https://archiveofourown.org/tags/C++%20Programming/works", FeedKind::Author).unwrap();
        assert_eq!(plus.name, "C++ Programming");
        assert_eq!(plus.url(1), "https://archiveofourown.org/tags/C%2B%2B%20Programming/works");
        assert!(listing_problem("<p>This tag has not been marked common and can't be filtered on (yet).</p>").is_some());
    }
}
