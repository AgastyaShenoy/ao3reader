//! Followed authors, tags and collections, kept in `follows.json` next to the library.
//! Nothing is ever followed automatically: feeds are only added when the user asks.

use crate::ao3::models::WorkMeta;
use crate::ao3::search::{Blurb, Feed, FeedKind, Listing};
use crate::storage::config::AppConfig;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Unread works kept per feed; older ones are dropped.
const MAX_UNREAD: usize = 100;
/// Feeds are checked again automatically after this long.
pub const STALE_MINUTES: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Followed {
    pub feed: Feed,
    pub added: DateTime<Utc>,
    /// When the feed was last fetched successfully.
    #[serde(default)]
    pub checked: Option<DateTime<Utc>>,
    /// When a check was last tried, successful or not (so a failing feed isn't retried
    /// in a loop).
    #[serde(default)]
    pub attempted: Option<DateTime<Utc>>,
    /// First page of the feed at the last check, newest first.
    #[serde(default)]
    pub latest: Vec<Blurb>,
    #[serde(default)]
    pub total: Option<u64>,
    /// Work id → updated date at the last check; spots new and updated works.
    #[serde(default)]
    pub seen: HashMap<String, String>,
    /// Works new or updated since the user last looked, newest first.
    #[serde(default)]
    pub unread: Vec<String>,
    /// The unread works that were already known and got a new chapter or edit.
    #[serde(default)]
    pub updated: Vec<String>,
    /// Why the last check failed, if it did.
    #[serde(default)]
    pub error: Option<String>,
}

impl Followed {
    pub fn id(&self) -> String {
        self.feed.id()
    }

    pub fn mark(&self, work_id: &str) -> Mark {
        if !self.unread.iter().any(|w| w == work_id) {
            Mark::None
        } else if self.updated.iter().any(|w| w == work_id) {
            Mark::Updated
        } else {
            Mark::New
        }
    }
}

/// How a work in a feed is flagged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mark {
    None,
    Updated,
    New,
}

impl Mark {
    pub fn label(self) -> &'static str {
        match self {
            Mark::None => "",
            Mark::Updated => "Updated",
            Mark::New => "New",
        }
    }
}

/// Parses a blurb date ("09 Mar 2024") for sorting; unknown dates sort last.
pub fn blurb_date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s.trim(), "%d %b %Y").unwrap_or(NaiveDate::MIN)
}

/// Which feeds a view shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope<'a> {
    All,
    Kind(FeedKind),
    One(&'a str),
}

impl Scope<'_> {
    fn includes(&self, f: &Followed) -> bool {
        match self {
            Scope::All => true,
            Scope::Kind(k) => f.feed.kind == *k,
            Scope::One(id) => f.id() == *id,
        }
    }
}

pub struct FollowStore {
    follows: Vec<Followed>,
    path: PathBuf,
}

impl Default for FollowStore {
    fn default() -> Self {
        Self::load()
    }
}

impl FollowStore {
    pub fn load() -> Self {
        let path = AppConfig::get_app_dir().join("follows.json");
        let follows = crate::storage::safe::load_json(&path, true).unwrap_or_default();
        Self { follows, path }
    }

    pub fn save(&self) -> Result<(), String> {
        let json = serde_json::to_string(&self.follows).map_err(|e| e.to_string())?;
        crate::storage::safe::write_atomic(&self.path, json.as_bytes()).map_err(|e| format!("Couldn't save follows: {e}"))
    }

    pub fn list(&self) -> &[Followed] {
        &self.follows
    }

    pub fn get(&self, id: &str) -> Option<&Followed> {
        self.follows.iter().find(|f| f.id() == id)
    }

    pub fn is_following(&self, kind: FeedKind, name: &str) -> bool {
        self.follows.iter().any(|f| f.feed.kind == kind && f.feed.name.eq_ignore_ascii_case(name))
    }

    pub fn add(&mut self, feed: Feed) -> Result<String, String> {
        if self.is_following(feed.kind, &feed.name) {
            return Err(format!("You already follow {}.", feed.name));
        }
        let id = feed.id();
        self.follows.push(Followed {
            feed,
            added: Utc::now(),
            checked: None,
            attempted: None,
            latest: Vec::new(),
            total: None,
            seen: HashMap::new(),
            unread: Vec::new(),
            updated: Vec::new(),
            error: None,
        });
        self.save()?;
        Ok(id)
    }

    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        self.follows.retain(|f| f.id() != id);
        self.save()
    }

    /// Clears the unread marks of the feeds in `scope`.
    pub fn mark_read(&mut self, scope: Scope) -> Result<(), String> {
        for f in self.follows.iter_mut().filter(|f| scope.includes(f)) {
            f.unread.clear();
            f.updated.clear();
        }
        self.save()
    }

    /// Clears one work's unread mark everywhere (e.g. after opening it).
    pub fn mark_work_read(&mut self, work_id: &str) -> Result<(), String> {
        let mut changed = false;
        for f in &mut self.follows {
            let before = f.unread.len();
            f.unread.retain(|w| w != work_id);
            f.updated.retain(|w| w != work_id);
            changed |= f.unread.len() != before;
        }
        if changed {
            self.save()?;
        }
        Ok(())
    }

    /// Stores a fresh first page and returns how many works are new or updated.
    /// The first check of a feed only sets the baseline, so nothing counts as new.
    pub fn record(&mut self, id: &str, listing: Listing) -> Result<usize, String> {
        let Some(f) = self.follows.iter_mut().find(|f| f.id() == id) else {
            return Ok(0);
        };
        let first = f.checked.is_none();
        let mut fresh = Vec::new();
        for b in &listing.works {
            let wid = &b.meta.id;
            let known = f.seen.get(wid);
            if first || known.is_some_and(|d| *d == b.updated) {
                continue;
            }
            if known.is_some() && !f.unread.contains(wid) {
                f.updated.push(wid.clone());
            }
            if !f.unread.contains(wid) {
                fresh.push(wid.clone());
            }
        }
        let count = fresh.len();
        fresh.extend(f.unread.drain(..));
        fresh.truncate(MAX_UNREAD);
        f.updated.retain(|w| fresh.contains(w));
        f.unread = fresh;
        f.seen = listing.works.iter().map(|b| (b.meta.id.clone(), b.updated.clone())).collect();
        f.latest = listing.works;
        f.total = listing.total;
        f.checked = Some(Utc::now());
        f.attempted = f.checked;
        f.error = None;
        self.save()?;
        Ok(count)
    }

    pub fn record_error(&mut self, id: &str, error: String) -> Result<(), String> {
        if let Some(f) = self.follows.iter_mut().find(|f| f.id() == id) {
            f.error = Some(error);
            f.attempted = Some(Utc::now());
        }
        self.save()
    }

    /// Distinct unread works in `scope`.
    pub fn unread_count(&self, scope: Scope) -> usize {
        let mut ids: Vec<&String> = self.follows.iter().filter(|f| scope.includes(f)).flat_map(|f| &f.unread).collect();
        ids.sort();
        ids.dedup();
        ids.len()
    }

    pub fn unread_total(&self) -> usize {
        self.unread_count(Scope::All)
    }

    /// The newest works of the feeds in `scope`, de-duplicated, newest update first,
    /// each with the feeds it came from and its strongest mark.
    pub fn timeline(&self, scope: Scope) -> Vec<(&Blurb, Vec<&Followed>, Mark)> {
        let mut out: Vec<(&Blurb, Vec<&Followed>, Mark)> = Vec::new();
        for f in self.follows.iter().filter(|f| scope.includes(f)) {
            for b in &f.latest {
                let mark = f.mark(&b.meta.id);
                match out.iter_mut().find(|(o, _, _)| o.meta.id == b.meta.id) {
                    Some(entry) => {
                        entry.1.push(f);
                        entry.2 = entry.2.max(mark);
                    }
                    None => out.push((b, vec![f], mark)),
                }
            }
        }
        out.sort_by(|a, b| blurb_date(&b.0.updated).cmp(&blurb_date(&a.0.updated)));
        out
    }

    /// Feeds whose last check (or failed attempt) is older than `max_age_minutes`.
    pub fn stale(&self, max_age_minutes: i64) -> Vec<String> {
        let cutoff = Utc::now() - chrono::Duration::minutes(max_age_minutes);
        self.follows.iter().filter(|f| f.attempted.or(f.checked).is_none_or(|c| c < cutoff)).map(|f| f.id()).collect()
    }

    /// Authors or tags that appear in the user's own works, most frequent first, leaving
    /// out what is already followed. Only ever offered as suggestions to tap.
    pub fn suggestions<'a>(&self, works: impl Iterator<Item = &'a WorkMeta>, kind: FeedKind, limit: usize) -> Vec<String> {
        // name → (works it appears in, is a relationship tag)
        let mut counts: HashMap<String, (usize, bool)> = HashMap::new();
        for m in works {
            let names: Vec<&String> = match kind {
                FeedKind::Author => m.authors.iter().collect(),
                // Ships first, then fandoms: they make the most useful feeds.
                FeedKind::Tag => m.relationships.iter().chain(m.fandoms.iter()).collect(),
                FeedKind::Collection => Vec::new(),
            };
            for n in names {
                let n = n.trim();
                let skip = n.is_empty() || n == "Anonymous" || n.contains("orphan_account") || n.eq_ignore_ascii_case("Unknown");
                if !skip {
                    let e = counts.entry(n.to_string()).or_default();
                    e.0 += 1;
                    e.1 |= m.relationships.iter().any(|r| r.trim() == n);
                }
            }
        }
        let mut list: Vec<(String, (usize, bool))> = counts.into_iter().filter(|(n, _)| !self.is_following(kind, n)).collect();
        // Most frequent first; on a tie ships before fandoms, then by name.
        list.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(b.1 .1.cmp(&a.1 .1)).then_with(|| a.0.cmp(&b.0)));
        list.into_iter().take(limit).map(|(n, _)| n).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blurb(id: &str, updated: &str) -> Blurb {
        Blurb { meta: WorkMeta { id: id.into(), ..WorkMeta::default() }, updated: updated.into() }
    }

    fn listing(works: Vec<Blurb>) -> Listing {
        Listing { works, total: None, page: 1, has_next: false }
    }

    fn store(dir: &std::path::Path) -> FollowStore {
        FollowStore { follows: Vec::new(), path: dir.join("follows.json") }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ao3-follows-{name}-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn new_and_updated_works() {
        let dir = temp("marks");
        let mut s = store(&dir);
        let id = s.add(Feed { kind: FeedKind::Tag, name: "Harmony".into() }).unwrap();
        assert!(s.add(Feed { kind: FeedKind::Tag, name: "harmony".into() }).is_err());

        // The first check is the baseline.
        assert_eq!(s.record(&id, listing(vec![blurb("1", "01 Jan 2024"), blurb("2", "02 Jan 2024")])).unwrap(), 0);
        // A new work and an updated one.
        let n = s.record(&id, listing(vec![blurb("3", "05 Jan 2024"), blurb("1", "04 Jan 2024"), blurb("2", "02 Jan 2024")])).unwrap();
        assert_eq!(n, 2);
        let f = s.get(&id).unwrap();
        assert_eq!(f.unread, vec!["3", "1"]);
        assert_eq!((f.mark("3"), f.mark("1"), f.mark("2")), (Mark::New, Mark::Updated, Mark::None));
        assert_eq!(s.unread_total(), 2);

        let t = s.timeline(Scope::All);
        assert_eq!(t.iter().map(|x| x.0.meta.id.as_str()).collect::<Vec<_>>(), vec!["3", "1", "2"]);
        assert_eq!(t[0].2, Mark::New);

        s.mark_work_read("3").unwrap();
        assert_eq!(s.get(&id).unwrap().unread, vec!["1"]);
        s.mark_read(Scope::All).unwrap();
        assert_eq!(s.unread_total(), 0);
        assert!(s.get(&id).unwrap().updated.is_empty());

        let reloaded: Vec<Followed> = serde_json::from_str(&fs::read_to_string(dir.join("follows.json")).unwrap()).unwrap();
        assert_eq!(reloaded.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scopes_staleness_and_suggestions() {
        let dir = temp("scopes");
        let mut s = store(&dir);
        let a = s.add(Feed { kind: FeedKind::Author, name: "ana".into() }).unwrap();
        let t = s.add(Feed { kind: FeedKind::Tag, name: "Fluff".into() }).unwrap();
        s.record(&a, listing(vec![blurb("1", "01 Jan 2024")])).unwrap();
        s.record(&t, listing(vec![blurb("2", "01 Jan 2024")])).unwrap();
        s.record(&a, listing(vec![blurb("5", "02 Jan 2024"), blurb("1", "01 Jan 2024")])).unwrap();
        assert_eq!(s.unread_count(Scope::Kind(FeedKind::Author)), 1);
        assert_eq!(s.unread_count(Scope::Kind(FeedKind::Tag)), 0);
        assert_eq!(s.timeline(Scope::Kind(FeedKind::Tag)).len(), 1);

        // A failed check counts as an attempt, so it isn't retried straight away.
        let c = s.add(Feed { kind: FeedKind::Collection, name: "c".into() }).unwrap();
        assert_eq!(s.stale(30), vec![c.clone()]);
        s.record_error(&c, "AO3 is down".into()).unwrap();
        assert!(s.stale(30).is_empty());

        let w1 = WorkMeta { authors: vec!["ana".into(), "bo".into()], relationships: vec!["A/B".into()], fandoms: vec!["F".into()], ..WorkMeta::default() };
        let w2 = WorkMeta { authors: vec!["bo".into()], relationships: vec!["A/B".into()], fandoms: vec!["F".into(), "G".into()], ..WorkMeta::default() };
        let w3 = WorkMeta { authors: vec!["Anonymous".into()], ..WorkMeta::default() };
        let works = [w1, w2, w3];
        // "ana" is already followed; "Anonymous" is never suggested.
        assert_eq!(s.suggestions(works.iter(), FeedKind::Author, 5), vec!["bo"]);
        assert_eq!(s.suggestions(works.iter(), FeedKind::Tag, 5), vec!["A/B", "F", "G"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
