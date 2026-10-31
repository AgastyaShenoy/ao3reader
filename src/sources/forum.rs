//! Story forums with threadmarks (XenForo): SpaceBattles and Sufficient Velocity. A story is a
//! thread whose author marks the posts that are chapters ("threadmarks"). The app reads it the way
//! the forum's own reader mode shows it, ten chapters a page, and keeps up with new chapters
//! through the thread's RSS feed, which carries the newest chapters' text.

use super::opds::{Entry, Feed, Link, STORY_MIME};
use super::{day, xml, CatalogClient};
use crate::ao3::models::{Chapter, Work, WorkMeta};
use crate::ao3::parser::Ao3Parser;
use crate::ao3::search::{Blurb, ItemLink};
use crate::reader::rich_text::paragraph_words;
use scraper::{ElementRef, Html, Selector};
use std::collections::HashSet;
use std::time::Duration;

pub struct Site {
    pub name: &'static str,
    pub host: &'static str,
    /// Starts the library ids of its stories ("sb-1340351").
    pub tag: &'static str,
    /// Boards to browse: (name, path, what's in it).
    pub boards: &'static [(&'static str, &'static str, &'static str)],
}

pub const SITES: [Site; 2] = [
    Site {
        name: "SpaceBattles",
        host: "forums.spacebattles.com",
        tag: "sb",
        boards: &[
            ("Creative Writing", "forums/creative-writing.18", "Fan fiction and original stories, newest threads first."),
            ("Worm", "forums/worm.115", "Stories set in Worm, the web serial."),
            ("Quests", "forums/quests.240", "Stories whose readers vote on what happens next."),
        ],
    },
    Site {
        name: "Sufficient Velocity",
        host: "forums.sufficientvelocity.com",
        tag: "sv",
        boards: &[
            ("User Fiction", "forums/user-fiction.2", "Fan fiction and original stories, newest threads first."),
            ("Quests", "forums/quests.29", "Stories whose readers vote on what happens next."),
        ],
    },
];

/// Reader pages fetched at most for one story (ten chapters each).
const MAX_PAGES: u32 = 200;
/// A pause between pages, to go easy on the forum.
const PAUSE: Duration = Duration::from_millis(800);

pub fn site_of(url: &str) -> Option<&'static Site> {
    let host = super::host(url);
    SITES.iter().find(|s| host == s.host)
}

/// A site's start page in Browse.
pub fn root_url(site: &Site) -> String {
    format!("https://{}/", site.host)
}

/// A thread's own address ("https://host/threads/slug.123/") from any link into it.
pub fn thread_url(url: &str) -> Option<String> {
    let site = site_of(url)?;
    let path = url.split(site.host).nth(1)?;
    let mut parts = path.split(['?', '#']).next()?.split('/').filter(|p| !p.is_empty());
    if parts.next()? != "threads" {
        return None;
    }
    let slug = parts.next()?;
    let id = slug.rsplit('.').next()?;
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(format!("https://{}/threads/{slug}/", site.host))
}

/// The library id of a thread's story: "sb-1340351".
pub fn story_id(url: &str) -> Option<String> {
    let site = site_of(url)?;
    let thread = thread_url(url)?;
    let slug = thread.trim_end_matches('/').rsplit('/').next()?;
    let id = slug.rsplit('.').next()?.to_string();
    Some(format!("{}-{id}", site.tag))
}

/// A story's thread from its library id ("sb-1340351"): the forum sends the short address on
/// to the thread.
pub fn url_of_id(id: &str) -> Option<String> {
    let (tag, n) = id.split_once('-')?;
    let site = SITES.iter().find(|s| s.tag == tag)?;
    (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).then(|| format!("https://{}/threads/{n}/", site.host))
}

/// The page of a forum the catalog screen shows: the site's boards, or a board's newest
/// threads (each a story to get).
pub fn feed(client: &CatalogClient, url: &str) -> Result<Feed, String> {
    let site = site_of(url).ok_or("Not a story forum.")?;
    if url.trim_end_matches('/') == root_url(site).trim_end_matches('/') {
        return Ok(root_feed(site));
    }
    board_feed(&client.page(url)?, site)
}

fn root_feed(site: &Site) -> Feed {
    Feed {
        title: site.name.into(),
        entries: site
            .boards
            .iter()
            .map(|(name, path, what)| Entry {
                id: format!("https://{}/{path}/", site.host),
                title: (*name).into(),
                summary: (*what).into(),
                links: vec![Link { rel: "subsection".into(), href: board_rss(site, path), mime: "application/atom+xml".into(), title: String::new() }],
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn board_rss(site: &Site, path: &str) -> String {
    format!("https://{}/{path}/index.rss", site.host)
}

/// A board's newest threads, from its RSS feed.
fn board_feed(rss: &str, site: &Site) -> Result<Feed, String> {
    let root = xml::parse(rss).map_err(|e| format!("{} sent something the app can't read: {e}", site.name))?;
    let channel = root.first("channel").ok_or("Not a forum feed.")?;
    let entries = channel
        .all("item")
        .filter_map(|it| {
            let link = thread_url(&it.child_text("link")?)?;
            let summary = it.first("encoded").map(|c| teaser(&c.raw_text())).unwrap_or_default();
            Some(Entry {
                id: link.clone(),
                title: it.child_text("title").unwrap_or_else(|| "Untitled".into()),
                authors: it.child_text("creator").into_iter().collect(),
                summary,
                updated: it.child_text("pubDate").map(|d| day(&d)).unwrap_or_default(),
                links: vec![Link { rel: "http://opds-spec.org/acquisition".into(), href: link, mime: STORY_MIME.into(), title: String::new() }],
                ..Default::default()
            })
        })
        .collect();
    Ok(Feed { title: channel.child_text("title").unwrap_or_else(|| site.name.into()), entries, ..Default::default() })
}

/// The start of a story's first post, as a short description.
fn teaser(html: &str) -> String {
    let doc = Html::parse_fragment(html);
    let text: String = paragraphs(doc.root_element()).into_iter().filter(|p| p != "Read more").collect::<Vec<_>>().join(" ");
    let text = text.replace('*', "");
    match text.char_indices().nth(280) {
        Some((i, _)) => format!("{}…", text[..i].trim_end()),
        None => text,
    }
}

/// Paragraphs of a post. Forum posts break lines with `<br>` rather than paragraphs: a blank
/// line ends a paragraph. Italic, bold and struck-out text keep their markers, as in AO3 works.
pub(crate) fn paragraphs(el: ElementRef) -> Vec<String> {
    let mut text = String::new();
    walk(el, &mut text);
    let text = Ao3Parser::collapse_whitespace(&text);
    let mut out = Vec::new();
    for part in text.split("\n\n") {
        let p = part.trim_matches(|c: char| c.is_whitespace());
        if !p.is_empty() {
            out.push(p.lines().map(str::trim).collect::<Vec<_>>().join("\n"));
        }
    }
    out
}

fn walk(el: ElementRef, out: &mut String) {
    for node in el.children() {
        if let Some(t) = node.value().as_text() {
            out.extend(t.chars().map(|c| if c.is_ascii_whitespace() { ' ' } else { c }));
            continue;
        }
        let Some(child) = ElementRef::wrap(node) else { continue };
        let name = child.value().name();
        // "Click to expand…" under long quotes, scripts and pictures aren't part of the text.
        if child.value().classes().any(|c| c == "bbCodeBlock-expandLink" || c == "bbCodeBlock-shrinkLink") || matches!(name, "script" | "style" | "noscript" | "img" | "svg") {
            continue;
        }
        let marker = match name {
            "em" | "i" => Some("*"),
            "strong" | "b" => Some("**"),
            "s" | "strike" | "del" => Some("~~"),
            _ => None,
        };
        if let Some(m) = marker {
            let mut inner = String::new();
            walk(child, &mut inner);
            // Markers hug the text; spaces inside the tag move outside them.
            let core = inner.trim();
            if core.is_empty() {
                out.push_str(&inner);
            } else {
                out.push_str(&inner[..inner.len() - inner.trim_start().len()]);
                out.push_str(m);
                out.push_str(core);
                out.push_str(m);
                out.push_str(&inner[inner.trim_end().len()..]);
            }
            continue;
        }
        match name {
            "br" => out.push('\n'),
            "hr" => out.push_str("\n\n---\n\n"),
            "div" | "p" | "blockquote" | "ul" | "ol" | "li" | "table" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "button" | "aside" => {
                out.push_str("\n\n");
                walk(child, out);
                out.push_str("\n\n");
            }
            _ => walk(child, out),
        }
    }
}

/// One page of the forum's reader mode.
struct ReaderPage {
    title: String,
    author: String,
    pages: u32,
    chapters: Vec<Chapter>,
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("valid selector")
}

fn text_of(e: ElementRef) -> String {
    e.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_reader(html: &str) -> ReaderPage {
    let doc = Html::parse_document(html);
    let title = doc.select(&sel("h1.p-title-value")).next().map(text_of).unwrap_or_default();
    let author = doc
        .select(&sel(".p-description .username"))
        .next()
        .map(text_of)
        .or_else(|| doc.select(&sel("article.message")).next().and_then(|a| a.value().attr("data-author").map(str::to_string)))
        .unwrap_or_default();
    let pages = doc.select(&sel(".pageNav-main a")).filter_map(|a| text_of(a).parse::<u32>().ok()).max().unwrap_or(1);
    let (label, body) = (sel(".threadmarkLabel"), sel(".message-body .bbWrapper"));
    let chapters = doc
        .select(&sel("article.message.hasThreadmark"))
        .filter_map(|post| {
            let id = post.value().attr("data-content").map(|c| c.trim_start_matches("post-").to_string())?;
            let paras = paragraphs(post.select(&body).next()?);
            let mut ch = Chapter::new(0, post.select(&label).next().map(text_of).unwrap_or_default());
            ch.id = Some(id);
            ch.paragraphs = paras;
            Some(ch)
        })
        .collect();
    ReaderPage { title, author, pages, chapters }
}

/// A chapter from the thread's RSS feed: (post id, title, date, paragraphs).
struct FeedChapter {
    id: String,
    title: String,
    date: String,
    author: String,
    paragraphs: Vec<String>,
}

struct ThreadFeed {
    title: String,
    description: String,
    /// Newest first.
    chapters: Vec<FeedChapter>,
}

fn parse_thread_rss(rss: &str) -> Result<ThreadFeed, String> {
    let root = xml::parse(rss).map_err(|e| format!("The story's feed couldn't be read: {e}"))?;
    let channel = root.first("channel").ok_or("Not a story feed.")?;
    let chapters = channel
        .all("item")
        .filter_map(|it| {
            // "https://host/posts/126716826" or ".../#post-126716826".
            let guid = it.child_text("guid").or_else(|| it.child_text("link"))?;
            let id: String = guid.rsplit(['/', '-']).next()?.chars().filter(char::is_ascii_digit).collect();
            let html = it.first("encoded").map(xml::Element::raw_text).unwrap_or_default();
            let doc = Html::parse_fragment(&html);
            Some(FeedChapter {
                id,
                title: it.child_text("title").unwrap_or_default(),
                date: it.child_text("pubDate").map(|d| day(&d)).unwrap_or_default(),
                author: it.child_text("creator").unwrap_or_default(),
                paragraphs: paragraphs(doc.root_element()),
            })
        })
        .filter(|c| !c.id.is_empty())
        .collect();
    Ok(ThreadFeed { title: channel.child_text("title").unwrap_or_default(), description: channel.child_text("description").unwrap_or_default(), chapters })
}

/// Numbers the chapters and brings the work's word and chapter counts up to date.
fn finish(work: &mut Work) {
    for (i, c) in work.chapters.iter_mut().enumerate() {
        c.index = i + 1;
        if c.title.trim().is_empty() {
            c.title = format!("Chapter {}", i + 1);
        }
        c.word_count = c.paragraphs.iter().map(|p| paragraph_words(p)).sum();
    }
    let n = work.chapters.len() as u32;
    work.meta.stats.words = work.chapters.iter().map(|c| c.word_count).sum();
    work.meta.stats.chapters_published = n;
    work.fetched_at = chrono::Utc::now();
}

/// Reader pages `first..`, adding the chapters the work doesn't have yet. Returns how many.
fn read_pages(client: &CatalogClient, thread: &str, work: &mut Work, first: u32) -> Result<usize, String> {
    let mut known: HashSet<String> = work.chapters.iter().filter_map(|c| c.id.clone()).collect();
    let mut added = 0;
    let mut page = first.max(1);
    loop {
        if page > first.max(1) {
            std::thread::sleep(PAUSE);
        }
        let url = if page == 1 { format!("{thread}reader/") } else { format!("{thread}reader/page-{page}") };
        let p = parse_reader(&client.page(&url)?);
        if work.meta.title.is_empty() {
            work.meta.title = p.title.clone();
        }
        if work.meta.authors.is_empty() && !p.author.is_empty() {
            work.meta.authors = vec![p.author.clone()];
        }
        let found = p.chapters.len();
        for c in p.chapters {
            if c.id.as_ref().is_some_and(|id| known.insert(id.clone())) {
                work.chapters.push(c);
                added += 1;
            }
        }
        if found == 0 || page >= p.pages || page >= MAX_PAGES {
            break;
        }
        page += 1;
    }
    Ok(added)
}

/// Downloads a whole story: every threadmarked chapter, in order.
pub fn download(client: &CatalogClient, url: &str) -> Result<Work, String> {
    let thread = thread_url(url).ok_or("That isn't a link to a story thread.")?;
    let site = site_of(&thread).ok_or("Not a story forum.")?;
    let mut work = Work {
        meta: WorkMeta { id: story_id(&thread).unwrap_or_default(), title: String::new(), authors: Vec::new(), ..WorkMeta::default() },
        chapters: Vec::new(),
        fetched_at: chrono::Utc::now(),
        resources: Vec::new(),
    };
    work.meta.fandoms = Vec::new();
    work.meta.language = "English".into();
    work.meta.summary_text = format!("A story from {}.", site.name);
    read_pages(client, &thread, &mut work, 1)?;
    if work.chapters.is_empty() {
        return Err("This thread has no threadmarked chapters to read.".into());
    }
    if work.meta.title.is_empty() {
        work.meta.title = "Untitled story".into();
    }
    // The thread's own description, if its feed has one.
    if let Ok(feed) = client.page(&format!("{thread}threadmarks.rss")).and_then(|r| parse_thread_rss(&r)) {
        if !feed.description.trim().is_empty() {
            work.meta.summary_text = feed.description;
        }
    }
    finish(&mut work);
    Ok(work)
}

/// The story as a followed item: its latest chapter is what tells a change apart.
fn blurb(thread: &str, title: &str, author: &str, summary: &str, newest: Option<(&str, &str)>, chapters: u32) -> Blurb {
    let mut meta = WorkMeta { id: story_id(thread).unwrap_or_default(), title: title.to_string(), authors: Vec::new(), ..WorkMeta::default() };
    if !author.is_empty() {
        meta.authors = vec![author.to_string()];
    }
    meta.summary_text = summary.to_string();
    meta.stats.chapters_published = chapters;
    meta.fandoms = Vec::new();
    Blurb { meta, updated: newest.map(|n| n.1.to_string()).unwrap_or_default(), version: newest.map(|n| n.0.to_string()).unwrap_or_default(), link: Some(ItemLink::Story { url: thread.to_string() }) }
}

/// The followed item for a story just downloaded.
pub fn blurb_of(work: &Work, url: &str) -> Blurb {
    let thread = thread_url(url).unwrap_or_else(|| url.to_string());
    let newest = work.chapters.last().and_then(|c| c.id.as_deref());
    let date = chrono::Utc::now().format("%d %b %Y").to_string();
    blurb(&thread, &work.meta.title, work.meta.authors.first().map_or("", String::as_str), &work.meta.summary_text, newest.map(|id| (id, date.as_str())), work.chapters.len() as u32)
}

/// Checks a followed story for new chapters. The library's copy (`saved`) comes back with them
/// added, when there are any.
pub fn check(client: &CatalogClient, url: &str, saved: Option<Work>) -> Result<(Blurb, Option<Work>), String> {
    let thread = thread_url(url).ok_or("That isn't a link to a story thread.")?;
    let feed = parse_thread_rss(&client.page(&format!("{thread}threadmarks.rss"))?)?;
    let newest = feed.chapters.first();
    let author = newest.map_or("", |c| c.author.as_str());
    let mut updated = None;
    let mut count = saved.as_ref().map_or(0, |w| w.chapters.len() as u32);
    if let Some(mut work) = saved {
        let known: HashSet<String> = work.chapters.iter().filter_map(|c| c.id.clone()).collect();
        let fresh: Vec<&FeedChapter> = feed.chapters.iter().rev().filter(|c| !known.contains(&c.id)).collect();
        if !fresh.is_empty() {
            let caught_up = work.chapters.is_empty() || feed.chapters.iter().any(|c| known.contains(&c.id));
            if caught_up {
                for c in fresh {
                    let mut ch = Chapter::new(0, c.title.clone());
                    ch.id = Some(c.id.clone());
                    ch.paragraphs = c.paragraphs.clone();
                    work.chapters.push(ch);
                }
            } else {
                // More new chapters than the feed carries: read them from the reader pages,
                // starting at the page after the saved ones.
                let first = (work.chapters.len() / 10) as u32 + 1;
                read_pages(client, &thread, &mut work, first)?;
            }
            finish(&mut work);
            count = work.chapters.len() as u32;
            updated = Some(work);
        }
    }
    let title = if feed.title.is_empty() { updated.as_ref().map(|w| w.meta.title.clone()).unwrap_or_default() } else { feed.title.clone() };
    let b = blurb(&thread, &title, author, &feed.description, newest.map(|c| (c.id.as_str(), c.date.as_str())), count);
    Ok((b, updated))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_and_ids() {
        let t = "https://forums.spacebattles.com/threads/dragon-of-the-sun.1340351/page-3#post-126313196";
        assert_eq!(thread_url(t).unwrap(), "https://forums.spacebattles.com/threads/dragon-of-the-sun.1340351/");
        assert_eq!(story_id(t).unwrap(), "sb-1340351");
        assert_eq!(story_id("https://forums.sufficientvelocity.com/threads/136085/reader/").unwrap(), "sv-136085");
        assert_eq!(url_of_id("sv-136085").as_deref(), Some("https://forums.sufficientvelocity.com/threads/136085/"));
        assert_eq!(story_id(&url_of_id("sb-1340351").unwrap()).as_deref(), Some("sb-1340351"));
        assert!(url_of_id("xx-1").is_none() && url_of_id("sb-").is_none() && url_of_id("12345").is_none());
        assert!(thread_url("https://forums.spacebattles.com/forums/quests.240/").is_none());
        assert!(thread_url("https://example.com/threads/x.1/").is_none());
    }

    #[test]
    fn posts_become_paragraphs() {
        let html = r#"<div class="bbWrapper">First line<br />
still the first.<br />
<br />
<i>Second</i>, with <b>bold </b>text.<blockquote class="bbCodeBlock">A quote<div class="bbCodeBlock-expandLink">Click to expand...</div></blockquote>After.</div>"#;
        let doc = Html::parse_fragment(html);
        let p = paragraphs(doc.root_element());
        assert_eq!(p, ["First line\nstill the first.", "*Second*, with **bold** text.", "A quote", "After."]);
    }

    #[test]
    fn reader_pages_and_feeds() {
        let page = r#"<html><body><h1 class="p-title-value">Dragon of the Sun</h1>
<div class="p-description"><a class="username">OptimusMaximus</a></div>
<ul class="pageNav-main"><li><a>1</a></li><li><a>2</a></li><li><a>26</a></li></ul>
<article class="message message--post hasThreadmark" data-author="OptimusMaximus" data-content="post-111">
<span class="threadmarkLabel">Taiyang I</span><span class="threadmarkLabel">Taiyang I</span>
<div class="message-body"><div class="bbWrapper">One.<br /><br />Two.</div></div></article>
<article class="message message--post" data-content="post-112"><div class="message-body"><div class="bbWrapper">A comment.</div></div></article>
</body></html>"#;
        let p = parse_reader(page);
        assert_eq!((p.title.as_str(), p.author.as_str(), p.pages), ("Dragon of the Sun", "OptimusMaximus", 26));
        assert_eq!(p.chapters.len(), 1);
        assert_eq!((p.chapters[0].id.as_deref(), p.chapters[0].title.as_str()), (Some("111"), "Taiyang I"));
        assert_eq!(p.chapters[0].paragraphs, ["One.", "Two."]);

        let rss = r#"<?xml version="1.0"?><rss xmlns:content="http://purl.org/rss/1.0/modules/content/" xmlns:dc="http://purl.org/dc/elements/1.1/"><channel>
<title>Dragon of the Sun</title><description>A story.</description>
<item><title>Training Exercise</title><pubDate>Sat, 31 Oct 2026 03:34:57 +0000</pubDate><guid>https://forums.spacebattles.com/posts/113</guid><dc:creator>OptimusMaximus</dc:creator>
<content:encoded><![CDATA[<div class="bbWrapper">New <i>chapter</i>.<br /><br />End.</div>]]></content:encoded></item>
<item><title>Taiyang I</title><pubDate>Thu, 15 Oct 2026 23:56:45 +0000</pubDate><guid>https://forums.spacebattles.com/posts/111</guid>
<content:encoded><![CDATA[<div class="bbWrapper">One.</div>]]></content:encoded></item>
</channel></rss>"#;
        let f = parse_thread_rss(rss).unwrap();
        assert_eq!(f.chapters.len(), 2);
        assert_eq!((f.chapters[0].id.as_str(), f.chapters[0].date.as_str()), ("113", "31 Oct 2026"));
        assert_eq!(f.chapters[0].paragraphs, ["New *chapter*.", "End."]);
    }

    #[test]
    fn boards_list_threads_as_stories() {
        let rss = r#"<?xml version="1.0"?><rss xmlns:content="http://purl.org/rss/1.0/modules/content/" xmlns:dc="http://purl.org/dc/elements/1.1/"><channel><title>Creative Writing</title>
<item><title>I Reincarnated as an elf</title><pubDate>Sat, 31 Oct 2026 09:17:43 +0000</pubDate><link>https://forums.spacebattles.com/threads/i-reincarnated-as-an-elf.1337510/</link><dc:creator>jenny301</dc:creator>
<content:encoded><![CDATA[<div class="bbWrapper">The morning had begun.<br /><br /><a href="x">Read more</a></div>]]></content:encoded></item></channel></rss>"#;
        let f = board_feed(rss, &SITES[0]).unwrap();
        let e = &f.entries[0];
        assert_eq!((e.title.as_str(), e.authors[0].as_str(), e.summary.as_str()), ("I Reincarnated as an elf", "jenny301", "The morning had begun."));
        let book = e.book().unwrap();
        assert_eq!((book.mime.as_str(), book.href.as_str()), (STORY_MIME, "https://forums.spacebattles.com/threads/i-reincarnated-as-an-elf.1337510/"));
        assert_eq!(root_feed(&SITES[1]).entries.len(), 2);
    }

    /// Reads a short story from SpaceBattles: its reader pages and feed.
    #[test]
    #[ignore = "reaches SpaceBattles"]
    fn live_spacebattles() {
        let c = CatalogClient::new();
        let w = download(&c, "https://forums.spacebattles.com/threads/dragon-of-the-sun.1340351/").unwrap();
        println!("{} by {:?}: {} chapters, {} words", w.meta.title, w.meta.authors, w.chapters.len(), w.meta.stats.words);
        println!("first: {} / {:?}", w.chapters[0].title, w.chapters[0].paragraphs.first());
        let mut older = w.clone();
        older.chapters.pop();
        let (b, updated) = check(&c, "https://forums.spacebattles.com/threads/dragon-of-the-sun.1340351/", Some(older)).unwrap();
        println!("check: {} {} {:?}", b.meta.title, b.version, updated.map(|w| w.chapters.len()));
    }
}
