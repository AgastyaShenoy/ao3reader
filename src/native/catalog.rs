//! Browse screen: one place to find books. AO3 (its own search, drawn by `browse::SearchScreen`)
//! sits beside OPDS catalogs (Project Gutenberg, and any the reader adds), whose books download
//! into the library.

use super::gfx::*;
use super::icons;
use super::ui::*;
use crate::sources::opds::{Entry, Feed};
use crate::sources::{self, Catalog};
use std::collections::HashMap;

/// The same column as AO3's search below it.
const MAX_W: f32 = 980.0;

pub enum CatalogAction {
    /// Load a page of a catalog; `append` adds it under the current one (the "More" button).
    Load { catalog: Catalog, url: String, append: bool },
    Search { catalog: Catalog, feed: Feed, query: String },
    /// Download a book (`key` names it until it's in the library).
    Get { catalog: Catalog, url: String, key: String, title: String },
    Read(String),
    /// The reader's own catalogs changed.
    Save(Vec<Catalog>),
}

/// A book being downloaded, or its result.
pub enum Getting {
    Busy,
    Done(String),
    Failed(String),
}

struct Page {
    title: String,
    url: String,
    feed: Option<Feed>,
}

#[derive(Default)]
struct AddForm {
    name: String,
    url: String,
    user: String,
    password: String,
    error: Option<String>,
}

#[derive(Default)]
pub struct CatalogScreen {
    /// Which source: 0 is AO3, then the built-in catalogs, then the reader's.
    current: usize,
    stack: Vec<Page>,
    busy: bool,
    error: Option<String>,
    query: String,
    pub getting: HashMap<String, Getting>,
    adding: Option<AddForm>,
}

fn all(user: &[Catalog]) -> Vec<Catalog> {
    sources::builtin().into_iter().chain(user.iter().cloned()).collect()
}

impl CatalogScreen {
    /// A page arrived (or failed).
    pub fn loaded(&mut self, url: &str, append: bool, result: Result<Feed, String>) {
        self.busy = false;
        match result {
            Ok(feed) => {
                self.error = None;
                if let Some(top) = self.stack.last_mut().filter(|p| append || p.url == url) {
                    match (&mut top.feed, append) {
                        (Some(have), true) => {
                            have.entries.extend(feed.entries);
                            have.next = feed.next;
                        }
                        _ => {
                            if top.title.is_empty() {
                                top.title = feed.title.clone();
                            }
                            top.feed = Some(feed);
                        }
                    }
                }
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn open(&mut self, catalog: &Catalog, title: String, url: String, out: &mut Vec<CatalogAction>) {
        self.stack.push(Page { title, url: url.clone(), feed: None });
        self.busy = true;
        self.error = None;
        out.push(CatalogAction::Load { catalog: catalog.clone(), url, append: false });
    }

    pub fn on_ao3(&self) -> bool {
        self.current == 0 && self.adding.is_none()
    }

    pub fn show_ao3(&mut self) {
        self.adding = None;
        self.current = 0;
    }

    /// Opens the form for adding a catalog.
    pub fn start_adding(&mut self) {
        self.adding = Some(AddForm::default());
    }

    /// Back to AO3 (after a catalog was removed elsewhere).
    pub fn reset(&mut self) {
        self.adding = None;
        self.switch(0);
    }

    fn switch(&mut self, to: usize) {
        self.current = to;
        self.stack.clear();
        self.query.clear();
        self.error = None;
        self.busy = false;
    }

    /// The screen, with its sources along the top. When AO3 is the source, returns where its
    /// search goes (`ao3_count` is that search's "1,234 works").
    pub fn frame(&mut self, ui: &mut Ui, body: Rect, user: &[Catalog], ao3_count: Option<String>) -> (Vec<CatalogAction>, Option<Rect>) {
        let mut out = Vec::new();
        let c = ui.c;
        let catalogs = all(user);
        self.current = self.current.min(catalogs.len());

        let w = (body.w - 48.0).min(MAX_W);
        let x = body.x + (body.w - w) / 2.0;
        let t = ui.text("Browse", &TextStyle::ui(26.0).semibold(), x, body.y + 22.0, c.text);
        ui.text_opts(
            "Find books on AO3 and Project Gutenberg, or add your own catalogue: Calibre, Kavita, Komga, any OPDS feed.",
            &TextStyle::ui(SMALL),
            TextOpts::ellipsize((w - t.w - 20.0).max(60.0)),
            x + t.w + 16.0,
            body.y + 22.0 + t.h - 18.0,
            c.muted,
        );

        // Sources, and adding a catalog.
        let mut cx = x;
        let cy = body.y + 22.0 + t.h + 14.0;
        let builtin = sources::builtin().len();
        let names: Vec<&str> = std::iter::once("AO3").chain(catalogs.iter().map(|c| c.name.as_str())).collect();
        for (i, name) in names.iter().enumerate() {
            let sel = i == self.current && self.adding.is_none();
            let bw = ui.button_width(name, None);
            let r = Rect::new(cx, cy, bw, 34.0);
            if ui.button_ex(id2(id("cat-chip"), i), r, name, None, if sel { ButtonKind::Tonal } else { ButtonKind::Secondary }, true) && !sel {
                self.adding = None;
                self.switch(i);
            }
            cx += bw + 8.0;
        }
        let add_r = Rect::new(cx, cy, ui.button_width("Add a catalogue", Some("\u{E710}")), 34.0);
        if ui.button_ex(id("cat-add"), add_r, "Add a catalogue", Some("\u{E710}"), ButtonKind::Ghost, true) {
            self.adding = Some(AddForm::default());
        }
        if let Some(count) = ao3_count.filter(|_| self.on_ao3()) {
            ui.text_in(&count, &TextStyle::ui(SMALL), Rect::new(x, cy, w, 34.0), Align::Right, c.muted);
        }
        let mut y = cy + 50.0;

        if let Some(form) = &mut self.adding {
            let mut close = false;
            if let Some(added) = Self::add_form(ui, x, &mut y, w, form, &mut close) {
                let mut list = user.to_vec();
                list.push(added);
                let index = catalogs.len() + 1;
                out.push(CatalogAction::Save(list));
                self.adding = None;
                self.switch(index);
            } else if close {
                self.adding = None;
            }
            return (out, None);
        }
        if self.current == 0 {
            return (out, Some(Rect::new(body.x, y, body.w, (body.bottom() - y).max(0.0))));
        }
        let index = self.current - 1;
        let catalog = catalogs[index].clone();
        if self.stack.is_empty() {
            self.open(&catalog, catalog.name.clone(), catalog.url.clone(), &mut out);
        }

        // Where we are: back, the page's name, removing the reader's own catalog.
        let row = Rect::new(x, y, w, 34.0);
        let mut tx = x;
        if self.stack.len() > 1 {
            if ui.button_ex(id("cat-back"), Rect::new(x, y, 92.0, 34.0), "Back", Some("\u{E72B}"), ButtonKind::Ghost, true) {
                self.stack.pop();
                self.error = None;
                self.busy = self.stack.last().is_some_and(|p| p.feed.is_none());
            }
            tx += 100.0;
        }
        let title = self.stack.last().map(|p| p.title.clone()).unwrap_or_default();
        let mut right = row.right();
        if index >= builtin {
            let rw = ui.button_width("Remove", Some(icons::DELETE));
            right -= rw;
            if ui.button_ex(id("cat-remove"), Rect::new(right, y, rw, 34.0), "Remove", Some(icons::DELETE), ButtonKind::Ghost, true) {
                let mut list = user.to_vec();
                list.remove(index - builtin);
                out.push(CatalogAction::Save(list));
                self.switch(1);
                return (out, None);
            }
        }
        ui.text_in(&title, &TextStyle::ui(16.0).semibold(), Rect::new(tx, y, (right - tx - 12.0).max(0.0), 34.0), Align::Left, c.text);
        y += 44.0;

        // Search, when the catalog has it.
        let feed = self.stack.last().and_then(|p| p.feed.clone());
        let root_search = self.stack.first().and_then(|p| p.feed.as_ref()).and_then(|f| f.search.clone().map(|_| f.clone()));
        let searchable = feed.as_ref().filter(|f| f.search.is_some()).cloned().or(root_search);
        if let Some(sf) = searchable {
            let input = Rect::new(x, y, w - 44.0, 36.0);
            let resp = ui.text_input(id("cat-q"), input, &mut self.query, &format!("Search {}", catalog.name), false);
            let go = ui.icon_button(id("cat-go"), Rect::new(input.right() + 4.0, y, 40.0, 36.0), icons::SEARCH, "Search", false);
            if (resp.submitted || go) && !self.query.trim().is_empty() {
                let q = self.query.trim().to_string();
                let url = format!("search:{q}");
                self.stack.truncate(1);
                self.stack.push(Page { title: format!("“{q}”"), url: url.clone(), feed: None });
                self.busy = true;
                self.error = None;
                ui.scroll_to_top(id("cat-list"));
                out.push(CatalogAction::Search { catalog: catalog.clone(), feed: sf, query: q });
            }
            y += 50.0;
        }

        let list = Rect::new(body.x, y, body.w, (body.bottom() - y).max(0.0));
        if let Some(e) = &self.error {
            ui.notice(Rect::new(x, y, w, 64.0), e);
            return (out, None);
        }
        let Some(feed) = feed else {
            ui.spinner(x + w / 2.0, y + 40.0, 12.0);
            return (out, None);
        };
        if feed.entries.is_empty() {
            ui.text_in("Nothing here.", &TextStyle::ui(14.0), Rect::new(x, y, w, 40.0), Align::Center, c.muted);
            return (out, None);
        }

        let busy = self.busy;
        let getting = &self.getting;
        let mut opened: Option<(String, String)> = None;
        let mut get: Option<(String, String, String)> = None;
        let mut read: Option<String> = None;
        let mut more = false;
        ui.scroll_area(id("cat-list"), list, |ui, _view, y0| {
            let mut yy = y0;
            for (i, e) in feed.entries.iter().enumerate() {
                let h = Self::entry(ui, x, yy, w, i, e, getting, &mut opened, &mut get, &mut read);
                yy += h + 8.0;
            }
            if feed.next.is_some() {
                let r = Rect::new(x + (w - 140.0) / 2.0, yy + 4.0, 140.0, 36.0);
                if busy {
                    ui.spinner(r.x + r.w / 2.0, r.y + 18.0, 10.0);
                } else if ui.button(id("cat-more"), r, "More") {
                    more = true;
                }
                yy += 52.0;
            }
            yy + 24.0 - y0
        });
        if let Some((title, url)) = opened {
            self.open(&catalog, title, url, &mut out);
        }
        if let Some((key, url, title)) = get {
            self.getting.insert(key.clone(), Getting::Busy);
            out.push(CatalogAction::Get { catalog: catalog.clone(), url, key, title });
        }
        if let Some(id) = read {
            out.push(CatalogAction::Read(id));
        }
        if more {
            if let (Some(next), Some(top)) = (feed.next.clone(), self.stack.last()) {
                self.busy = true;
                out.push(CatalogAction::Load { catalog: catalog.clone(), url: next, append: true });
                let _ = top;
            }
        }
        (out, None)
    }

    /// One entry: a book to get, or a shelf to open. Returns its height.
    #[allow(clippy::too_many_arguments)]
    fn entry(
        ui: &mut Ui,
        x: f32,
        y: f32,
        w: f32,
        i: usize,
        e: &Entry,
        getting: &HashMap<String, Getting>,
        opened: &mut Option<(String, String)>,
        get: &mut Option<(String, String, String)>,
        read: &mut Option<String>,
    ) -> f32 {
        let c = ui.c;
        let btn_w = 120.0;
        let text_w = (w - btn_w - 48.0).max(120.0);
        let title = ui.measure(&e.title, &TextStyle::ui(15.0).semibold(), TextOpts::wrap(text_w).max_lines(2));
        let by = if e.authors.is_empty() { String::new() } else { e.authors.join(", ") };
        let sub = ui.measure(&by, &TextStyle::ui(13.0), TextOpts::ellipsize(text_w));
        let summary = if e.summary.is_empty() || e.summary == by { None } else { Some(ui.measure(&e.summary, &TextStyle::ui(12.5), TextOpts::wrap(text_w).max_lines(2))) };
        let h = (16.0 + title.h + if by.is_empty() { 0.0 } else { sub.h + 2.0 } + summary.map_or(0.0, |s| s.h + 6.0) + 14.0).max(64.0);
        let r = Rect::new(x, y, w, h);
        ui.card(r);
        let mut ty = y + 14.0;
        ui.g.draw_text(title, x + 16.0, ty, c.text);
        ty += title.h + 2.0;
        if !by.is_empty() {
            ui.g.draw_text(sub, x + 16.0, ty, c.muted);
            ty += sub.h + 6.0;
        }
        if let Some(s) = summary {
            ui.g.draw_text(s, x + 16.0, ty, c.text.alpha(0.82));
        }

        let br = Rect::new(r.right() - 16.0 - btn_w, y + (h - 34.0) / 2.0, btn_w, 34.0);
        let key = if e.id.is_empty() { e.title.clone() } else { e.id.clone() };
        if let Some(book) = e.book() {
            match getting.get(&key) {
                Some(Getting::Busy) => ui.spinner(br.x + br.w / 2.0, br.y + 17.0, 10.0),
                Some(Getting::Done(work)) => {
                    if ui.button_ex(id2(id("cat-read"), i), br, "Read", Some(icons::READING), ButtonKind::Tonal, true) {
                        *read = Some(work.clone());
                    }
                }
                other => {
                    let label = if matches!(other, Some(Getting::Failed(_))) { "Try again" } else { "Get" };
                    if ui.button_ex(id2(id("cat-get"), i), br, label, Some(icons::DOWNLOAD), ButtonKind::Tonal, true) {
                        *get = Some((key.clone(), book.href.clone(), e.title.clone()));
                    }
                    let kind = if book.mime.contains("pdf") { "PDF" } else { "EPUB" };
                    let tip = match other {
                        Some(Getting::Failed(msg)) => msg.clone(),
                        _ => format!("Download the {kind} to your library"),
                    };
                    ui.tip(id2(id("cat-get"), i), br, &tip);
                }
            }
        } else if let Some(feed) = e.feed() {
            let label = "Open";
            if ui.button_ex(id2(id("cat-open"), i), br, label, Some("\u{E76C}"), ButtonKind::Ghost, true) {
                *opened = Some((e.title.clone(), feed.href.clone()));
            }
        }
        h
    }

    /// The form for adding a catalog. Returns it once filled in.
    fn add_form(ui: &mut Ui, x: f32, y: &mut f32, w: f32, f: &mut AddForm, close: &mut bool) -> Option<Catalog> {
        let c = ui.c;
        let card = Rect::new(x, *y, w, 330.0);
        ui.card(card);
        ui.text("Add a catalogue", &TextStyle::ui(16.0).semibold(), x + 18.0, *y + 16.0, c.text);
        ui.text_opts(
            "The address of an OPDS feed: Calibre's content server (…/opds), Kavita, Komga, or a library's catalogue. A user name and password only if it asks for one.",
            &TextStyle::ui(SMALL),
            TextOpts::wrap(w - 36.0),
            x + 18.0,
            *y + 44.0,
            c.muted,
        );
        let fw = (w - 36.0 - 12.0) / 2.0;
        let mut fy = *y + 92.0;
        let label = |ui: &mut Ui, s: &str, lx: f32, ly: f32| {
            ui.text(s, &TextStyle::ui(12.5).semibold(), lx, ly, c.muted);
        };
        label(ui, "Name", x + 18.0, fy);
        label(ui, "Address", x + 18.0 + fw + 12.0, fy);
        fy += 20.0;
        ui.text_input(id("cat-add-name"), Rect::new(x + 18.0, fy, fw, 36.0), &mut f.name, "My library", false);
        let url = ui.text_input(id("cat-add-url"), Rect::new(x + 30.0 + fw, fy, fw, 36.0), &mut f.url, "https://…/opds", false);
        fy += 52.0;
        label(ui, "User name (optional)", x + 18.0, fy);
        label(ui, "Password (optional)", x + 18.0 + fw + 12.0, fy);
        fy += 20.0;
        ui.text_input(id("cat-add-user"), Rect::new(x + 18.0, fy, fw, 36.0), &mut f.user, "", false);
        let pass = ui.text_input(id("cat-add-pass"), Rect::new(x + 30.0 + fw, fy, fw, 36.0), &mut f.password, "", true);
        fy += 54.0;
        if let Some(e) = &f.error {
            ui.text(e, &TextStyle::ui(SMALL), x + 18.0, fy + 8.0, c.danger_text);
        }
        let add_r = Rect::new(card.right() - 18.0 - 96.0, fy, 96.0, 34.0);
        let cancel_r = Rect::new(add_r.x - 8.0 - 96.0, fy, 96.0, 34.0);
        if ui.button(id("cat-add-cancel"), cancel_r, "Cancel") {
            *close = true;
        }
        let submit = ui.button_ex(id("cat-add-ok"), add_r, "Add", None, ButtonKind::Primary, true) || url.submitted || pass.submitted;
        *y += card.h + 12.0;
        if !submit {
            return None;
        }
        let address = f.url.trim();
        if !(address.starts_with("http://") || address.starts_with("https://")) {
            f.error = Some("The address should start with http:// or https://".into());
            return None;
        }
        let name = if f.name.trim().is_empty() { address.split("://").nth(1).unwrap_or(address).split('/').next().unwrap_or(address).to_string() } else { f.name.trim().to_string() };
        Some(Catalog { name, url: address.to_string(), user: f.user.trim().to_string(), password: f.password.clone() })
    }
}
