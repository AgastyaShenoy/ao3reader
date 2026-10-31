//! Signing in to the story forums (SpaceBattles, Sufficient Velocity) and bringing in the
//! threads a reader watches there. Both run XenForo, whose sign-in is a plain form: the password
//! goes to the forum only, and what's kept is the forum's own "remember me" cookies.
//!
//! Royal Road isn't here: its pages ask for a browser check, which Flax Reader doesn't do.

use super::forum::{self, Site};
use crate::ao3::models::WorkMeta;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use ureq::http::Uri;
use ureq::{Agent, Cookie, ResponseExt};

/// Watched-thread pages read at most (twenty threads each).
const MAX_PAGES: u32 = 25;
/// A pause between pages, to go easy on the forum.
const PAUSE: Duration = Duration::from_millis(800);
const MAX_PAGE_BYTES: u64 = 8 * 1024 * 1024;

/// Who's signed in to a forum, and the cookies that keep them signed in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ForumAccount {
    pub host: String,
    pub username: String,
    #[serde(default)]
    pub cookies: Vec<(String, String)>,
}

/// A thread the reader watches: a story to add to the library.
#[derive(Debug, Clone, PartialEq)]
pub struct Thread {
    pub url: String,
    pub meta: WorkMeta,
}

/// A connection of its own for one forum, so its cookie jar holds that forum's sign-in only.
struct Session {
    agent: Agent,
    host: String,
}

impl Session {
    fn new(host: &str) -> Self {
        let agent: Agent = Agent::config_builder()
            .user_agent(super::user_agent())
            .max_redirects(10)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            .build()
            .into();
        Self { agent, host: host.to_string() }
    }

    fn restore(account: &ForumAccount) -> Self {
        let s = Self::new(&account.host);
        let uri: Uri = format!("https://{}/", account.host).parse().expect("forum address");
        let mut jar = s.agent.cookie_jar_lock();
        for (name, value) in &account.cookies {
            if let Ok(c) = Cookie::parse(format!("{name}={value}; Path=/"), &uri) {
                let _ = jar.insert(c, &uri);
            }
        }
        jar.release();
        s
    }

    fn cookies(&self) -> Vec<(String, String)> {
        let jar = self.agent.cookie_jar_lock();
        let list = jar.iter().map(|c| (c.name().to_string(), c.value().to_string())).collect();
        jar.release();
        list
    }

    /// (status, final address, page).
    fn read(&self, resp: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<(u16, String, String), String> {
        let mut resp = resp.map_err(|e| format!("Couldn't reach {}: {e}", self.host))?;
        let status = resp.status().as_u16();
        let url = resp.get_uri().to_string();
        let body = resp.body_mut().with_config().limit(MAX_PAGE_BYTES).read_to_string().map_err(|e| format!("The page stopped loading: {e}"))?;
        if (status == 403 || status == 503) && super::is_bot_check(&body) {
            return Err(format!("{} is asking for a browser check, which Flax Reader doesn't do. Try again later.", self.host));
        }
        Ok((status, url, body))
    }

    fn get(&self, path: &str) -> Result<(u16, String, String), String> {
        self.read(self.agent.get(&format!("https://{}{path}", self.host)).call())
    }

    fn post(&self, path: &str, form: &[(&str, &str)]) -> Result<(u16, String, String), String> {
        self.read(self.agent.post(&format!("https://{}{path}", self.host)).send_form(form.iter().copied()))
    }
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("valid selector")
}

fn text_of(e: ElementRef) -> String {
    e.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether a XenForo page was shown to a signed-in member (`<html data-logged-in="true">`).
fn logged_in(doc: &Html) -> Option<bool> {
    doc.select(&sel("html[data-logged-in]")).next().and_then(|h| h.value().attr("data-logged-in")).map(|v| v == "true")
}

/// The form's anti-forgery token.
fn token(doc: &Html) -> Option<String> {
    doc.select(&sel("input[name='_xfToken']")).find_map(|i| i.value().attr("value").filter(|v| !v.is_empty()).map(str::to_string))
}

/// The signed-in member's name, from the page's navigation bar.
fn member_name(doc: &Html) -> Option<String> {
    doc.select(&sel(".p-navgroup-link--user .p-navgroup-linkText, a.p-navgroup-link--user"))
        .map(text_of)
        .find(|t| !t.is_empty())
}

/// What a page says went wrong (a wrong password, say).
fn error_text(doc: &Html) -> Option<String> {
    doc.select(&sel(".blockMessage--error, .formRow-explain--error, div.error")).map(text_of).find(|t| !t.is_empty())
}

/// Signs in to a forum with a user name (or email) and password.
pub fn login(site: &Site, username: &str, password: &str) -> Result<ForumAccount, String> {
    let s = Session::new(site.host);
    let (_, _, page) = s.get("/login/")?;
    let tok = token(&Html::parse_document(&page)).ok_or_else(|| format!("{}'s sign-in page didn't load properly. Try again in a moment.", site.name))?;
    let root = forum::root_url(site);
    let (_, url, page) = s.post("/login/login", &[("login", username), ("password", password), ("remember", "1"), ("_xfToken", &tok), ("_xfRedirect", &root)])?;
    if url.contains("/login/two-step") {
        return Err(format!("This account uses two-step verification, which Flax Reader can't do yet. You can still read {} without signing in.", site.name));
    }
    let doc = Html::parse_document(&page);
    if logged_in(&doc) != Some(true) {
        return Err(match error_text(&doc) {
            Some(e) => format!("{} said: {e}", site.name),
            None => format!("{} didn't sign you in. Check the user name and password.", site.name),
        });
    }
    let name = member_name(&doc).unwrap_or_else(|| username.to_string());
    Ok(ForumAccount { host: site.host.to_string(), username: name, cookies: s.cookies() })
}

/// The threads on a list page (a board, or the watched-threads page: XenForo draws both the
/// same way), and whether there's a next page.
pub fn parse_thread_list(html: &str, site: &Site) -> (Vec<Thread>, bool) {
    let doc = Html::parse_document(html);
    let threads = doc
        .select(&sel(".structItem--thread"))
        .filter_map(|item| {
            // The title link is the last one in the title (prefix labels come first).
            let a = item.select(&sel(".structItem-title a[href*='/threads/']")).last()?;
            let href = a.value().attr("href")?;
            let url = forum::thread_url(&format!("https://{}{}", site.host, href.split(site.host).last().unwrap_or(href)))?;
            let mut meta = WorkMeta { id: forum::story_id(&url)?, title: text_of(a), authors: Vec::new(), ..WorkMeta::default() };
            meta.fandoms = Vec::new();
            meta.language = "English".into();
            if let Some(author) = item.value().attr("data-author").filter(|a| !a.is_empty()) {
                meta.authors = vec![author.to_string()];
            }
            meta.summary_text = format!("A story from {}.", site.name);
            Some(Thread { url, meta })
        })
        .collect();
    let next = doc.select(&sel(".pageNav-jump--next")).next().is_some();
    (threads, next)
}

/// Every thread the reader watches on a forum. The account comes back with its cookies as the
/// forum last set them.
pub fn watched_threads(account: &ForumAccount) -> Result<(Vec<Thread>, ForumAccount), String> {
    let site = forum::SITES.iter().find(|s| s.host == account.host).ok_or("Not a story forum.")?;
    let s = Session::restore(account);
    let mut out: Vec<Thread> = Vec::new();
    for page in 1..=MAX_PAGES {
        if page > 1 {
            std::thread::sleep(PAUSE);
        }
        let (status, _, html) = s.get(&format!("/watched/threads?page={page}"))?;
        // Signed out, the forum answers 403 "You must be logged-in to do that".
        if logged_in(&Html::parse_document(&html)) == Some(false) {
            return Err(format!("You've been signed out of {}. Sign in again under Accounts.", site.name));
        }
        if !(200..300).contains(&status) {
            return Err(format!("{} answered with an error ({status}).", site.name));
        }
        let (threads, next) = parse_thread_list(&html, site);
        for t in threads {
            if !out.iter().any(|o| o.meta.id == t.meta.id) {
                out.push(t);
            }
        }
        if !next {
            break;
        }
    }
    let mut account = account.clone();
    account.cookies = s.cookies();
    Ok((out, account))
}

/// Signs out on the forum too, so its cookies stop working.
pub fn logout(account: &ForumAccount) {
    let s = Session::restore(account);
    if let Ok((_, _, page)) = s.get("/") {
        let doc = Html::parse_document(&page);
        if let Some(link) = doc.select(&sel("a[href*='/logout/']")).find_map(|a| a.value().attr("href").map(str::to_string)) {
            let path = link.split(account.host.as_str()).last().unwrap_or(&link).to_string();
            let _ = s.get(&path.replace("&amp;", "&"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"<html data-logged-in="true"><body>
<div class="structItem structItem--thread js-threadListItem-1140820" data-author="Summertime Madness">
  <div class="structItem-title"><a href="/forums/x/?prefix_id=3" class="labelLink">Quest</a>
    <a href="/threads/trending-stories.1140820/" data-tp-primary="on">Trending Stories</a></div>
</div>
<div class="structItem structItem--thread js-threadListItem-99" data-author="">
  <div class="structItem-title"><a href="https://forums.spacebattles.com/threads/99/">Untitled-ish</a></div>
</div>
<div class="structItem structItem--thread"><div class="structItem-title"><a href="/members/someone.1/">not a thread</a></div></div>
<a class="pageNav-jump pageNav-jump--next" href="/watched/threads?page=2">Next</a>
</body></html>"#;

    #[test]
    fn reads_a_thread_list() {
        let site = &forum::SITES[0];
        let (threads, next) = parse_thread_list(LIST, site);
        assert!(next);
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].meta.id, "sb-1140820");
        assert_eq!(threads[0].meta.title, "Trending Stories");
        assert_eq!(threads[0].meta.authors, ["Summertime Madness"]);
        assert_eq!(threads[0].url, "https://forums.spacebattles.com/threads/trending-stories.1140820/");
        assert_eq!(threads[1].meta.id, "sb-99");
        assert!(threads[1].meta.authors.is_empty());
        assert_eq!(logged_in(&Html::parse_document(LIST)), Some(true));
    }

    #[test]
    fn reads_the_sign_in_form() {
        let page = r#"<html data-logged-in="false"><form action="/login/login" method="post">
<input type="hidden" name="_xfToken" value="" /><input type="hidden" name="_xfToken" value="17934,abc" />
<input type="text" name="login" /><input type="password" name="password" /></form>
<div class="blockMessage blockMessage--error">Incorrect password. Please try again.</div></html>"#;
        let doc = Html::parse_document(page);
        assert_eq!(token(&doc).as_deref(), Some("17934,abc"));
        assert_eq!(logged_in(&doc), Some(false));
        assert_eq!(error_text(&doc).as_deref(), Some("Incorrect password. Please try again."));
    }

    /// The live sign-in page still has the form this module fills in.
    #[test]
    #[ignore]
    fn live_sign_in_form() {
        for site in &forum::SITES {
            let s = Session::new(site.host);
            let (_, _, page) = s.get("/login/").unwrap();
            let doc = Html::parse_document(&page);
            assert!(token(&doc).is_some(), "{}", site.name);
            assert_eq!(logged_in(&doc), Some(false));
            assert!(page.contains("action=\"/login/login\""));
        }
    }

    /// A public board lists threads the same way the watched-threads page does.
    #[test]
    #[ignore]
    fn live_board_list() {
        let site = &forum::SITES[0];
        let s = Session::new(site.host);
        let (_, _, page) = s.get("/forums/creative-writing.18/").unwrap();
        let (threads, next) = parse_thread_list(&page, site);
        assert!(threads.len() > 10 && next, "{} threads", threads.len());
        assert!(threads.iter().all(|t| t.meta.id.starts_with("sb-") && !t.meta.title.is_empty()));
    }
}
