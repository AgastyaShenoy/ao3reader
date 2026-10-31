//! Comments on AO3 works: reading a chapter's (or a one-shot's) comments, and the comment form
//! a signed-in reader posts with.

use scraper::{ElementRef, Html, Selector};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Comment {
    pub id: String,
    /// The commenter, "Name (Guest)" for a guest; empty for a deleted comment.
    pub author: String,
    /// "06 Jan 2025".
    pub date: String,
    /// "Chapter 3", on a whole work's comments.
    pub chapter: String,
    /// Paragraphs of the comment's text.
    pub paragraphs: Vec<String>,
    /// 0 for a comment on the work, 1 for a reply to it, and so on.
    pub depth: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct CommentPage {
    pub comments: Vec<Comment>,
    pub page: u32,
    pub has_next: bool,
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("valid selector")
}

fn text_of(e: ElementRef) -> String {
    e.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The comments on a page shown with `show_comments=true`, replies under what they answer.
pub fn parse_comments(html: &str, page: u32) -> CommentPage {
    let doc = Html::parse_document(html);
    let Some(root) = doc.select(&sel("#comments_placeholder")).next() else {
        return CommentPage { page, ..Default::default() };
    };
    let comments = root
        .select(&sel("li.comment"))
        .map(|li| {
            // Each level of replies sits in another `ol.thread`.
            let depth = li.ancestors().filter_map(ElementRef::wrap).filter(|e| e.value().name() == "ol" && e.value().classes().any(|c| c == "thread")).count().saturating_sub(1);
            let heading = li.select(&sel("h4.heading.byline")).next();
            let author = heading
                .map(|h| {
                    let linked = h.select(&sel("a[href*='/users/']")).next().map(text_of);
                    // A guest's name is plain text before the chapter and date.
                    linked.unwrap_or_else(|| h.text().next().unwrap_or("").trim().to_string())
                })
                .unwrap_or_default();
            let chapter = li.select(&sel("h4.heading.byline span.parent")).next().map(text_of).map(|t| t.trim_start_matches("on ").to_string()).unwrap_or_default();
            let date = li
                .select(&sel("h4.heading.byline span.posted"))
                .next()
                .map(|p| {
                    let part = |css: &str| p.select(&sel(css)).next().map(text_of).unwrap_or_default();
                    [part("span.date"), part("abbr.month"), part("span.year")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ")
                })
                .unwrap_or_default();
            let paragraphs = li
                .select(&sel(":scope > blockquote.userstuff"))
                .next()
                .map(|q| {
                    let ps: Vec<String> = q.select(&sel("p")).map(text_of).filter(|t| !t.is_empty()).collect();
                    if ps.is_empty() { vec![text_of(q)].into_iter().filter(|t| !t.is_empty()).collect() } else { ps }
                })
                .unwrap_or_else(|| {
                    // A deleted or hidden comment shows only a note.
                    let note = li.select(&sel("p")).next().map(text_of).unwrap_or_default();
                    if note.is_empty() { Vec::new() } else { vec![note] }
                });
            let id = li.value().id().unwrap_or("").trim_start_matches("comment_").to_string();
            Comment { id, author, date, chapter, paragraphs, depth }
        })
        .filter(|c| !c.paragraphs.is_empty())
        .collect();
    let has_next = root.select(&sel("ol.pagination li.next a[href]")).next().is_some();
    CommentPage { comments, page: page.max(1), has_next }
}

/// The comment form on a page, filled in with `text`: where it posts, and its fields. The
/// reader's default pseud is kept as AO3 has it selected.
pub fn comment_form(html: &str, text: &str) -> Option<(String, Vec<(String, String)>)> {
    let doc = Html::parse_document(html);
    let form = doc.select(&sel("form[id^='comment_for_'], form.new_comment, form[action$='/comments']")).find(|f| f.select(&sel("textarea[name='comment[comment_content]']")).next().is_some())?;
    let action = form.value().attr("action")?.to_string();
    let mut fields: Vec<(String, String)> = Vec::new();
    for input in form.select(&sel("input[name]")) {
        let v = input.value();
        let kind = v.attr("type").unwrap_or("text");
        if kind == "submit" || kind == "checkbox" && v.attr("checked").is_none() {
            continue;
        }
        fields.push((v.attr("name")?.to_string(), v.attr("value").unwrap_or("").to_string()));
    }
    for select in form.select(&sel("select[name]")) {
        let chosen = select.select(&sel("option[selected]")).next().or_else(|| select.select(&sel("option")).next());
        if let Some(o) = chosen {
            fields.push((select.value().attr("name")?.to_string(), o.value().attr("value").unwrap_or("").to_string()));
        }
    }
    fields.push(("comment[comment_content]".into(), text.to_string()));
    fields.push(("commit".into(), "Comment".into()));
    Some((action, fields))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<html><body><div id="comments_placeholder">
<h3 class="heading">Comments (3)</h3>
<ol class="thread">
  <li class="odd comment group" id="comment_501" role="article">
    <h4 class="heading byline"><a href="/users/reader/pseuds/reader">reader</a> <span class="parent">on Chapter 2</span>
      <span class="posted datetime"><abbr class="day" title="Monday">Mon</abbr> <span class="date">06</span> <abbr class="month" title="January">Jan</abbr> <span class="year">2025</span> <span class="time">10:15PM</span></span></h4>
    <blockquote class="userstuff"><p>Loved this chapter!</p><p>More please.</p></blockquote>
  </li>
  <li>
    <ol class="thread">
      <li class="even comment group" id="comment_502" role="article">
        <h4 class="heading byline">Anon Fan (Guest) <span class="posted datetime"><span class="date">07</span> <abbr class="month">Jan</abbr> <span class="year">2025</span></span></h4>
        <blockquote class="userstuff">Same here</blockquote>
      </li>
    </ol>
  </li>
  <li class="odd comment group" id="comment_503" role="article"><p>(Previous comment deleted.)</p></li>
</ol>
<ol class="pagination actions"><li class="next"><a rel="next" href="/works/1?page=2&show_comments=true">Next</a></li></ol>
<form id="comment_for_77" class="new_comment" action="/chapters/77/comments" method="post">
  <input type="hidden" name="authenticity_token" value="tok" />
  <select name="comment[pseud_id]"><option value="1">reader</option><option value="2" selected="selected">alt</option></select>
  <textarea name="comment[comment_content]"></textarea>
  <input type="hidden" name="controller_name" value="chapters" />
  <input type="submit" name="commit" value="Comment" />
</form>
</div></body></html>"#;

    #[test]
    fn reads_comments_and_replies() {
        let p = parse_comments(PAGE, 1);
        assert!(p.has_next);
        assert_eq!(p.comments.len(), 3);
        let first = &p.comments[0];
        assert_eq!((first.id.as_str(), first.author.as_str(), first.chapter.as_str(), first.date.as_str(), first.depth), ("501", "reader", "Chapter 2", "06 Jan 2025", 0));
        assert_eq!(first.paragraphs, ["Loved this chapter!", "More please."]);
        let reply = &p.comments[1];
        assert_eq!((reply.author.as_str(), reply.depth), ("Anon Fan (Guest)", 1));
        assert_eq!(reply.paragraphs, ["Same here"]);
        assert_eq!(p.comments[2].paragraphs, ["(Previous comment deleted.)"]);
        assert_eq!(parse_comments("<html><body>nothing</body></html>", 2).comments.len(), 0);
    }

    #[test]
    fn fills_in_the_comment_form() {
        let (action, fields) = comment_form(PAGE, "Hello!").unwrap();
        assert_eq!(action, "/chapters/77/comments");
        let get = |k: &str| fields.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("authenticity_token"), Some("tok"));
        assert_eq!(get("comment[pseud_id]"), Some("2"));
        assert_eq!(get("controller_name"), Some("chapters"));
        assert_eq!(get("comment[comment_content]"), Some("Hello!"));
        assert_eq!(get("commit"), Some("Comment"));
        assert!(comment_form("<form action='/x'></form>", "hi").is_none());
    }
}
