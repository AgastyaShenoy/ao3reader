//! OPDS catalogs (OPDS 1.x, Atom feeds): what a feed lists, where its books download from, its
//! next page and its search. Used for Project Gutenberg and any catalog a reader adds (Calibre's
//! content server, Kavita, Komga, Standard Ebooks for patrons, …).

use super::xml::{self, Element};

/// A link from a feed or an entry, with its address made absolute.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Link {
    pub rel: String,
    pub href: String,
    pub mime: String,
    pub title: String,
}

impl Link {
    /// A downloadable copy of a book.
    fn is_acquisition(&self) -> bool {
        self.rel.starts_with("http://opds-spec.org/acquisition")
    }

    /// Another feed: a shelf, a list, the next level down.
    fn is_feed(&self) -> bool {
        self.mime.starts_with("application/atom+xml") && !self.rel.contains("opds-spec.org/image") && !self.is_acquisition()
    }
}

/// Book formats the reader opens, most preferred first, and whether they're the richer version
/// ("EPUB3 with images" over a plain EPUB).
const FORMATS: [&str; 2] = ["application/epub+zip", "application/pdf"];

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    /// The description, as plain text.
    pub summary: String,
    pub language: String,
    pub links: Vec<Link>,
}

impl Entry {
    /// The best copy to download that the reader can open: EPUB before PDF, and the version
    /// with pictures when there's a choice.
    pub fn book(&self) -> Option<&Link> {
        FORMATS.iter().find_map(|mime| {
            let mut copies = self.links.iter().filter(|l| l.is_acquisition() && l.mime.starts_with(mime));
            let first = copies.next()?;
            let with_pictures = |l: &&Link| {
                let (href, title) = (l.href.to_lowercase(), l.title.to_lowercase());
                (href.contains("images") || title.contains("images")) && !href.contains("noimages") && !title.contains("no images")
            };
            Some(std::iter::once(first).chain(copies).find(with_pictures).unwrap_or(first))
        })
    }

    /// Where the entry leads when it isn't a book itself (or a book's own page, as Project
    /// Gutenberg's search results are).
    pub fn feed(&self) -> Option<&Link> {
        let feeds = || self.links.iter().filter(|l| l.is_feed());
        feeds().find(|l| l.rel == "subsection" || l.rel.is_empty() || l.rel == "alternate").or_else(|| feeds().next())
    }

    pub fn cover(&self) -> Option<&Link> {
        let find = |rel: &str| self.links.iter().find(|l| l.rel == rel);
        find("http://opds-spec.org/image/thumbnail").or_else(|| find("http://opds-spec.org/image"))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Feed {
    pub title: String,
    pub entries: Vec<Entry>,
    /// The next page of entries.
    pub next: Option<String>,
    /// How to search: an OpenSearch description to fetch (`Search::Description`) or a URL
    /// template with `{searchTerms}`.
    pub search: Option<Search>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Search {
    Description(String),
    Template(String),
}

/// Makes `href` absolute against `base` (the feed's own address).
pub fn resolve(base: &str, href: &str) -> String {
    let href = href.trim();
    if href.contains("://") || href.starts_with("mailto:") {
        return href.to_string();
    }
    let scheme_end = base.find("://").map_or(0, |i| i + 3);
    let scheme = &base[..scheme_end.saturating_sub(3)];
    if let Some(rest) = href.strip_prefix("//") {
        return format!("{scheme}://{rest}");
    }
    let host_end = base[scheme_end..].find('/').map_or(base.len(), |i| scheme_end + i);
    let origin = &base[..host_end];
    if href.starts_with('/') {
        return format!("{origin}{href}");
    }
    // Relative to the base's folder, without its query.
    let path = base[host_end..].split(['?', '#']).next().unwrap_or("");
    let dir = &path[..path.rfind('/').map_or(0, |i| i + 1)];
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    let (rel_path, tail) = match href.find(['?', '#']) {
        Some(i) => (&href[..i], &href[i..]),
        None => (href, ""),
    };
    for seg in rel_path.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    let trailing = if rel_path.ends_with('/') { "/" } else { "" };
    format!("{origin}/{}{trailing}{tail}", parts.join("/"))
}

fn links(el: &Element, base: &str) -> Vec<Link> {
    el.all("link")
        .filter_map(|l| {
            let href = l.attr("href")?;
            Some(Link {
                rel: l.attr("rel").unwrap_or_default().to_string(),
                href: resolve(base, href),
                mime: l.attr("type").unwrap_or_default().to_string(),
                title: l.attr("title").unwrap_or_default().to_string(),
            })
        })
        .collect()
}

/// Plain text from an entry's summary or content, which may be escaped HTML or XHTML.
fn description(el: &Element) -> String {
    let Some(c) = el.first("summary").or_else(|| el.first("content")) else { return String::new() };
    let raw = if c.attr("type") == Some("html") {
        // Escaped HTML: unescaped once by the XML reader, so it's markup now.
        strip_tags(&c.text())
    } else {
        c.text()
    };
    let joined = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    // Tags became spaces: none before punctuation.
    let mut out = String::with_capacity(joined.len());
    let mut chars = joined.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ' ' && chars.peek().is_some_and(|n| matches!(n, '.' | ',' | ';' | ':' | '!' | '?' | ')')) {
            continue;
        }
        out.push(c);
    }
    out
}

/// Text from HTML: block tags become spaces, inline ones (`<em>`, `<wbr>`, …) vanish.
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let Some(gt) = rest[lt..].find('>') else {
            rest = &rest[lt..];
            break;
        };
        let tag = &rest[lt + 1..lt + gt];
        let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        if !xml::INLINE.contains(&name.as_str()) {
            out.push(' ');
        }
        rest = &rest[lt + gt + 1..];
    }
    out.push_str(rest);
    xml::unescape(&out)
}

pub fn parse_feed(text: &str, base: &str) -> Result<Feed, String> {
    let root = xml::parse(text).map_err(|e| format!("Not a catalog feed: {e}"))?;
    if root.local() != "feed" {
        return Err("Not a catalog feed (OPDS 1).".into());
    }
    let feed_links = links(&root, base);
    let search = feed_links.iter().find(|l| l.rel == "search").map(|l| {
        if l.href.contains("{searchTerms}") {
            Search::Template(l.href.clone())
        } else if l.mime.contains("opensearchdescription") {
            Search::Description(l.href.clone())
        } else {
            Search::Template(l.href.clone())
        }
    });
    let entries = root
        .all("entry")
        .map(|e| Entry {
            id: e.child_text("id").unwrap_or_default(),
            title: e.child_text("title").unwrap_or_else(|| "Untitled".into()),
            authors: e.all("author").filter_map(|a| a.child_text("name")).collect(),
            summary: description(e),
            language: e.child_text("language").unwrap_or_default(),
            links: links(e, base),
        })
        .collect();
    Ok(Feed {
        title: root.child_text("title").unwrap_or_default(),
        entries,
        next: feed_links.iter().find(|l| l.rel == "next").map(|l| l.href.clone()),
        search,
    })
}

/// The search address template in an OpenSearch description, preferring catalog results.
pub fn parse_opensearch(text: &str, base: &str) -> Option<String> {
    let root = xml::parse(text).ok()?;
    let urls: Vec<&Element> = root.all("Url").collect();
    let pick = urls
        .iter()
        .find(|u| u.attr("type").is_some_and(|t| t.contains("atom+xml")))
        .or_else(|| urls.first())?;
    Some(resolve(base, pick.attr("template")?))
}

/// A search address from a template: `{searchTerms}` filled in, other optional parameters
/// dropped.
pub fn fill_template(template: &str, query: &str) -> String {
    let mut out = template.replace("{searchTerms}", &encode(query));
    // Optional parameters ("{startPage?}") are left empty.
    while let (Some(a), Some(b)) = (out.find('{'), out.find('}')) {
        if b < a {
            break;
        }
        out.replace_range(a..=b, "");
    }
    out
}

/// Percent-encodes a query for a URL.
pub fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>https://www.gutenberg.org/ebooks/search.opds/?query=austen</id>
  <title>Books matching "austen"</title>
  <link rel="search" type="application/opensearchdescription+xml" href="https://www.gutenberg.org/catalog/osd-books.xml"/>
  <link rel="next" type="application/atom+xml;profile=opds-catalog" href="/ebooks/search.opds/?query=austen&amp;start_index=26"/>
  <entry>
    <title>Pride and Prejudice</title>
    <content type="text">Jane Austen</content>
    <id>https://www.gutenberg.org/ebooks/1342.opds</id>
    <link type="application/atom+xml;profile=opds-catalog" rel="subsection" href="/ebooks/1342.opds"/>
    <link type="image/jpeg" rel="http://opds-spec.org/image/thumbnail" href="/cache/epub/1342/pg1342.cover.small.jpg"/>
  </entry>
</feed>"#;

    const BOOK: &str = r#"<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Pride and Prejudice</title>
  <entry>
    <title>Pride and Prejudice</title>
    <author><name>Austen, Jane</name></author>
    <summary type="html">&lt;p&gt;A novel of &lt;em&gt;manners&lt;/em&gt;.&lt;/p&gt;</summary>
    <link type="application/x-mobipocket-ebook" rel="http://opds-spec.org/acquisition" href="/ebooks/1342.kf8.images"/>
    <link type="application/epub+zip" rel="http://opds-spec.org/acquisition" title="EPUB (no images)" href="/ebooks/1342.epub.noimages"/>
    <link type="application/epub+zip" rel="http://opds-spec.org/acquisition" title="EPUB (with images)" href="/ebooks/1342.epub.images"/>
  </entry>
</feed>"#;

    #[test]
    fn search_results_lead_to_book_pages() {
        let f = parse_feed(SEARCH, "https://www.gutenberg.org/ebooks/search.opds/?query=austen").unwrap();
        assert_eq!(f.entries.len(), 1);
        let e = &f.entries[0];
        assert_eq!(e.summary, "Jane Austen");
        assert!(e.book().is_none());
        assert_eq!(e.feed().unwrap().href, "https://www.gutenberg.org/ebooks/1342.opds");
        assert_eq!(e.cover().unwrap().href, "https://www.gutenberg.org/cache/epub/1342/pg1342.cover.small.jpg");
        assert_eq!(f.next.as_deref(), Some("https://www.gutenberg.org/ebooks/search.opds/?query=austen&start_index=26"));
        assert_eq!(f.search, Some(Search::Description("https://www.gutenberg.org/catalog/osd-books.xml".into())));
    }

    #[test]
    fn a_book_page_offers_the_epub_with_pictures() {
        let f = parse_feed(BOOK, "https://www.gutenberg.org/ebooks/1342.opds").unwrap();
        let e = &f.entries[0];
        assert_eq!(e.authors, ["Austen, Jane"]);
        assert_eq!(e.summary, "A novel of manners.");
        assert_eq!(e.book().unwrap().href, "https://www.gutenberg.org/ebooks/1342.epub.images");
    }

    #[test]
    fn opensearch_and_templates() {
        let osd = r#"<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">
            <Url type="text/html" template="https://example.org/s?q={searchTerms}"/>
            <Url type="application/atom+xml" template="/search.opds/?query={searchTerms}&amp;page={startPage?}"/>
        </OpenSearchDescription>"#;
        let t = parse_opensearch(osd, "https://example.org/osd.xml").unwrap();
        assert_eq!(fill_template(&t, "pride & joy"), "https://example.org/search.opds/?query=pride+%26+joy&page=");
    }

    #[test]
    fn relative_addresses() {
        assert_eq!(resolve("https://h.org/a/b/feed.xml?x=1", "c.epub"), "https://h.org/a/b/c.epub");
        assert_eq!(resolve("https://h.org/a/b/", "../c/?p=2"), "https://h.org/a/c/?p=2");
        assert_eq!(resolve("https://h.org/a/b", "/root"), "https://h.org/root");
        assert_eq!(resolve("https://h.org/a", "//cdn.org/x"), "https://cdn.org/x");
    }
}
