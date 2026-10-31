//! Royal Road, followed through each story's RSS feed. Its chapter pages ask for a browser check,
//! which the app doesn't do, so new chapters open in the browser.

use super::{day, xml, CatalogClient};
use crate::ao3::models::WorkMeta;
use crate::ao3::search::{Blurb, ItemLink};

/// The story's number, from any link to it ("royalroad.com/fiction/21220/mother-of-learning",
/// or its feed).
pub fn fiction_id(url: &str) -> Option<u64> {
    let host = super::host(url);
    if host != "royalroad.com" && host != "www.royalroad.com" {
        return None;
    }
    let path = url.split(host).nth(1)?.split(['?', '#']).next()?;
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        ["fiction", "syndication", id, ..] | ["fiction", id, ..] | ["syndication", id, ..] => id.parse().ok(),
        _ => None,
    }
}

pub fn story_url(id: u64) -> String {
    format!("https://www.royalroad.com/fiction/{id}")
}

fn feed_url(id: u64) -> String {
    format!("https://www.royalroad.com/fiction/syndication/{id}")
}

/// The story as a followed item: its newest chapter, to open in the browser.
pub fn check(client: &CatalogClient, url: &str) -> Result<Blurb, String> {
    let id = fiction_id(url).ok_or("That isn't a link to a Royal Road story.")?;
    parse(&client.page(&feed_url(id))?, id)
}

fn parse(rss: &str, id: u64) -> Result<Blurb, String> {
    let root = xml::parse(rss.trim_start_matches('\u{feff}')).map_err(|e| format!("Royal Road's feed couldn't be read: {e}"))?;
    let channel = root.first("channel").ok_or("Not a Royal Road feed.")?;
    let title = channel.child_text("title").unwrap_or_else(|| "Untitled story".into());
    let newest = channel.first("item");
    // Items are titled "Story - Chapter".
    let chapter = newest
        .and_then(|i| i.child_text("title"))
        .map(|t| t.strip_prefix(&format!("{title} - ")).map(str::to_string).unwrap_or(t))
        .unwrap_or_default();
    let mut meta = WorkMeta { id: format!("rr-{id}"), title, authors: Vec::new(), ..WorkMeta::default() };
    meta.fandoms = Vec::new();
    meta.summary_text = if chapter.is_empty() { String::new() } else { format!("Latest chapter: {chapter}") };
    let link = newest.and_then(|i| i.child_text("link")).unwrap_or_else(|| story_url(id));
    Ok(Blurb {
        meta,
        updated: newest.and_then(|i| i.child_text("pubDate")).map(|d| day(&d)).unwrap_or_default(),
        version: newest.and_then(|i| i.child_text("guid").or_else(|| i.child_text("link"))).unwrap_or_default(),
        link: Some(ItemLink::Web { url: link }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_and_feed() {
        assert_eq!(fiction_id("https://www.royalroad.com/fiction/21220/mother-of-learning"), Some(21220));
        assert_eq!(fiction_id("https://royalroad.com/fiction/syndication/21220"), Some(21220));
        assert_eq!(fiction_id("https://www.royalroad.com/fiction/chapter/1269041"), None);
        assert_eq!(fiction_id("https://example.com/fiction/1"), None);

        let rss = "\u{feff}<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>Mother of Learning</title>\
<item><title>Mother of Learning - Afterword</title><link>https://www.royalroad.com/fiction/chapter/1269041</link><guid isPermaLink=\"false\">1269041</guid><pubDate>Thu, 06 Jul 2023 17:47:41 GMT</pubDate></item>\
<item><title>Mother of Learning - Chapter 108</title><link>https://www.royalroad.com/fiction/chapter/1</link><guid>1</guid></item></channel></rss>";
        let b = parse(rss, 21220).unwrap();
        assert_eq!((b.meta.id.as_str(), b.meta.title.as_str(), b.meta.summary_text.as_str()), ("rr-21220", "Mother of Learning", "Latest chapter: Afterword"));
        assert_eq!((b.updated.as_str(), b.version.as_str()), ("06 Jul 2023", "1269041"));
        assert_eq!(b.link, Some(ItemLink::Web { url: "https://www.royalroad.com/fiction/chapter/1269041".into() }));
    }
}
