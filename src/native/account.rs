//! Accounts: signing in to each site Flax Reader reads from (AO3, SpaceBattles, Sufficient
//! Velocity), and bringing in what the reader keeps there: AO3 bookmarks and Marked for Later,
//! and the threads they watch on the forums.

use super::gfx::*;
use super::icons;
use super::ui::*;
use crate::ao3::client::UserList;
use crate::ao3::models::UserSession;
use crate::sources::accounts::ForumAccount;
use crate::sources::forum::SITES;
use std::collections::{HashMap, HashSet};

const MAX_W: f32 = 720.0;

pub enum AccountAction {
    Login { username: String, password: String },
    ApplyCookie(String),
    Logout,
    /// Bring in one of the reader's AO3 lists.
    ImportAo3(UserList),
    ForumLogin { host: String, username: String, password: String },
    ForumLogout(String),
    /// Bring in the threads the reader watches on a forum (by its host).
    ImportWatched(String),
    /// Settings › Sources, where catalogue sign-ins are.
    GoSources,
}

#[derive(Default)]
struct SignIn {
    username: String,
    password: String,
}

/// The card for AO3.
pub const AO3: &str = "ao3";

#[derive(Default)]
pub struct AccountScreen {
    forms: HashMap<String, SignIn>,
    cookie: String,
    show_cookie: bool,
    /// Cards with something under way (by `AO3` or a forum's host).
    busy: HashSet<String>,
    /// The latest word on each card: (message, is_error).
    messages: HashMap<String, (String, bool)>,
}

impl AccountScreen {
    /// Shows a result on a card (by `AO3` or a forum's host), and stops its spinner.
    pub fn set_message(&mut self, card: &str, msg: impl Into<String>, error: bool) {
        self.busy.remove(card);
        self.messages.insert(card.to_string(), (msg.into(), error));
        if !error {
            if let Some(f) = self.forms.get_mut(card) {
                f.password.clear();
            }
            self.cookie.clear();
        }
    }

    fn start(&mut self, card: &str) {
        self.busy.insert(card.to_string());
        self.messages.remove(card);
    }

    pub fn frame(&mut self, ui: &mut Ui, body: Rect, session: &UserSession, forums: &[ForumAccount]) -> Vec<AccountAction> {
        let mut out = Vec::new();
        let c = ui.c;
        let w = (body.w - 48.0).min(MAX_W);
        let x = body.x + (body.w - w) / 2.0;
        let t = ui.text("Accounts", &TextStyle::ui(26.0).semibold(), x, body.y + 22.0, c.text);
        ui.text_opts(
            "Sign in to the sites you read on, and bring in what you keep there.",
            &TextStyle::ui(SMALL),
            TextOpts::ellipsize((w - t.w - 20.0).max(60.0)),
            x + t.w + 16.0,
            body.y + 22.0 + t.h - 18.0,
            c.muted,
        );
        let top = body.y + 22.0 + t.h + 16.0;
        let area = Rect::new(body.x, top, body.w, (body.bottom() - top).max(0.0));
        ui.scroll_area(id("account-scroll"), area, |ui, _view, y0| {
            let mut col = Col::new(x, y0 + 2.0, w);
            self.ao3_card(ui, &mut col, session, &mut out);
            for site in &SITES {
                let account = forums.iter().find(|a| a.host == site.host);
                self.forum_card(ui, &mut col, site.name, site.host, account, &mut out);
            }
            self.others(ui, &mut col, &mut out);
            col.y + 24.0 - y0
        });
        out
    }

    /// A card's title, status line and circle; returns where its body starts.
    fn head(ui: &mut Ui, r: Rect, name: &str, status: &str, signed_in: bool) -> f32 {
        let c = ui.c;
        ui.card(r);
        ui.g.fill_circle(r.x + 36.0, r.y + 38.0, 18.0, if signed_in { c.accent } else { c.border });
        ui.text_in(icons::CONTACT, &TextStyle::icons(15.0), Rect::new(r.x + 18.0, r.y + 20.0, 36.0, 36.0), Align::Center, if signed_in { c.on_accent } else { c.text });
        ui.text(name, &TextStyle::ui(16.0).semibold(), r.x + 68.0, r.y + 16.0, c.text);
        ui.text_opts(status, &TextStyle::ui(13.0), TextOpts::ellipsize(r.w - 210.0), r.x + 68.0, r.y + 40.0, c.muted);
        r.y + 76.0
    }

    /// The latest message on a card, if any, at `y`; returns its height.
    fn message(&self, ui: &mut Ui, card: &str, x: f32, y: f32, w: f32, draw: bool) -> f32 {
        let Some((msg, err)) = self.messages.get(card) else { return 0.0 };
        let c = ui.c;
        let t = ui.measure(msg, &TextStyle::ui(13.0), TextOpts::wrap(w - 28.0).line_height(1.35));
        if draw {
            let r = Rect::new(x, y, w, t.h + 18.0);
            let tone = if *err { c.danger } else { c.success };
            ui.g.fill_round(r, 8.0, tone.alpha(0.1));
            ui.g.stroke_round(r, 8.0, 1.0, tone.alpha(0.5));
            ui.g.draw_text(t, r.x + 14.0, r.y + 9.0, c.text);
        }
        t.h + 30.0
    }

    /// User name and password boxes and Sign in, at `y`. Returns the name and password when the
    /// reader signs in.
    fn sign_in_form(&mut self, ui: &mut Ui, card: &str, x: f32, y: f32, w: f32, who: &str) -> Option<(String, String)> {
        let c = ui.c;
        let busy = self.busy.contains(card);
        let form = self.forms.entry(card.to_string()).or_default();
        let half = (w - 12.0) / 2.0;
        ui.text(who, &TextStyle::ui(12.5), x, y, c.muted);
        ui.text("Password", &TextStyle::ui(12.5), x + half + 12.0, y, c.muted);
        let u = ui.text_input(id2(id("acct-user"), card), Rect::new(x, y + 20.0, half, 36.0), &mut form.username, "", false);
        let p = ui.text_input(id2(id("acct-pass"), card), Rect::new(x + half + 12.0, y + 20.0, half, 36.0), &mut form.password, "", true);
        let can = !busy && !form.username.trim().is_empty() && !form.password.is_empty();
        let bw = ui.button_width("Sign in", Some(icons::CONTACT)).max(110.0);
        let clicked = ui.button_ex(id2(id("acct-login"), card), Rect::new(x, y + 68.0, bw, 36.0), "Sign in", Some(icons::CONTACT), ButtonKind::Primary, can);
        if busy {
            ui.spinner(x + bw + 22.0, y + 86.0, 7.0);
            ui.text("Signing in…", &TextStyle::ui(13.0), x + bw + 38.0, y + 77.0, c.muted);
        }
        if can && (clicked || u.submitted || p.submitted) {
            let pair = (form.username.trim().to_string(), form.password.clone());
            self.start(card);
            return Some(pair);
        }
        None
    }

    /// A signed-in card's actions: Sign out on the head row, and buttons to bring lists in.
    fn signed_in_actions(&mut self, ui: &mut Ui, card: &str, r: Rect, y: f32, imports: &[(&str, &str)], out: &mut Vec<AccountAction>, on: impl Fn(usize) -> AccountAction) -> f32 {
        let c = ui.c;
        if ui.button_ex(id2(id("acct-logout"), card), Rect::new(r.right() - 120.0, r.y + 20.0, 104.0, 34.0), "Sign out", None, ButtonKind::Secondary, true) {
            out.push(if card == AO3 { AccountAction::Logout } else { AccountAction::ForumLogout(card.to_string()) });
        }
        let busy = self.busy.contains(card);
        let mut bx = r.x + 18.0;
        for (i, (label, _)) in imports.iter().enumerate() {
            let bw = ui.button_width(label, Some(icons::IMPORT));
            if ui.button_ex(id2(id2(id("acct-import"), card), i), Rect::new(bx, y, bw, 34.0), label, Some(icons::IMPORT), ButtonKind::Tonal, !busy) {
                self.start(card);
                out.push(on(i));
            }
            bx += bw + 8.0;
        }
        if busy {
            ui.spinner(bx + 12.0, y + 17.0, 7.0);
            ui.text("Bringing them in…", &TextStyle::ui(13.0), bx + 28.0, y + 8.0, c.muted);
        }
        let note = imports.iter().map(|(_, n)| *n).collect::<Vec<_>>().join(" ");
        let t = ui.measure(&note, &TextStyle::ui(12.5), TextOpts::wrap(r.w - 36.0).line_height(1.4));
        ui.g.draw_text(t, r.x + 18.0, y + 44.0, c.muted);
        44.0 + t.h
    }

    fn ao3_card(&mut self, ui: &mut Ui, col: &mut Col, session: &UserSession, out: &mut Vec<AccountAction>) {
        let c = ui.c;
        let has_cookie = session.session_cookie.is_some() || session.remember_token.is_some();
        let signed_in = session.is_logged_in && has_cookie;
        let imports = [
            ("Import bookmarks", "Bookmarks become favourites, in the collection “AO3 bookmarks”."),
            ("Import Marked for Later", "Marked for Later goes on your For later shelf. Works download when you open them."),
        ];
        let w = col.w;
        let body_h = if signed_in {
            ui.measure_only(|ui| {
                let t = ui.measure(&imports.iter().map(|(_, n)| *n).collect::<Vec<_>>().join(" "), &TextStyle::ui(12.5), TextOpts::wrap(w - 36.0).line_height(1.4));
                44.0 + t.h
            })
        } else if self.show_cookie {
            204.0
        } else {
            150.0
        };
        let msg_h = self.message(ui, AO3, 0.0, 0.0, w - 36.0, false);
        let r = col.take(76.0 + body_h + msg_h + 18.0);
        col.gap(12.0);
        let status = if signed_in {
            format!("Signed in as {}. Restricted works download, and you can comment.", session.username.as_deref().unwrap_or("an AO3 user"))
        } else if session.is_logged_in {
            "Your sign-in expired. Please sign in again.".to_string()
        } else {
            "Not signed in. Public works download without an account.".to_string()
        };
        let mut y = Self::head(ui, r, "Archive of Our Own", &status, signed_in);
        if signed_in {
            y += self.signed_in_actions(ui, AO3, r, y, &imports, out, |i| AccountAction::ImportAo3(if i == 0 { UserList::Bookmarks } else { UserList::MarkedForLater }));
        } else {
            if session.is_logged_in && ui.button_ex(id("acct-logout-stale"), Rect::new(r.right() - 120.0, r.y + 20.0, 104.0, 34.0), "Sign out", None, ButtonKind::Secondary, true) {
                out.push(AccountAction::Logout);
            }
            if let Some((username, password)) = self.sign_in_form(ui, AO3, r.x + 18.0, y, r.w - 36.0, "Username or email") {
                out.push(AccountAction::Login { username, password });
            }
            let mut show = self.show_cookie;
            ui.toggle(id("acct-cookie-toggle"), r.x + 18.0, y + 118.0, &mut show);
            self.show_cookie = show;
            ui.text("Use a session cookie instead", &TextStyle::ui(13.5), r.x + 70.0, y + 117.0, c.text);
            if self.show_cookie {
                let cr = Rect::new(r.x + 18.0, y + 154.0, r.w - 36.0 - 112.0, 36.0);
                let cresp = ui.text_input(id("acct-cookie"), cr, &mut self.cookie, "Value of the _otw_remember_me cookie", true);
                let can_apply = !self.busy.contains(AO3) && !self.cookie.trim().is_empty();
                let apply = ui.button_ex(id("acct-cookie-apply"), Rect::new(cr.right() + 8.0, cr.y, 104.0, 36.0), "Apply", None, ButtonKind::Secondary, can_apply);
                if can_apply && (apply || cresp.submitted) {
                    self.start(AO3);
                    out.push(AccountAction::ApplyCookie(self.cookie.trim().to_string()));
                }
            }
            y += body_h;
        }
        self.message(ui, AO3, r.x + 18.0, y + 12.0, r.w - 36.0, true);
    }

    fn forum_card(&mut self, ui: &mut Ui, col: &mut Col, name: &str, host: &str, account: Option<&ForumAccount>, out: &mut Vec<AccountAction>) {
        let imports = [("Import watched threads", "The threads you watch become stories in a collection named after the forum. Each downloads when you open it.")];
        let w = col.w;
        let body_h = if account.is_some() {
            ui.measure_only(|ui| 44.0 + ui.measure(imports[0].1, &TextStyle::ui(12.5), TextOpts::wrap(w - 36.0).line_height(1.4)).h)
        } else {
            110.0
        };
        let msg_h = self.message(ui, host, 0.0, 0.0, w - 36.0, false);
        let r = col.take(76.0 + body_h + msg_h + 18.0);
        col.gap(12.0);
        let status = match account {
            Some(a) => format!("Signed in as {}.", a.username),
            None => "Not signed in. Stories read and download without an account.".to_string(),
        };
        let mut y = Self::head(ui, r, name, &status, account.is_some());
        if account.is_some() {
            let h = host.to_string();
            y += self.signed_in_actions(ui, host, r, y, &imports, out, move |_| AccountAction::ImportWatched(h.clone()));
        } else {
            if let Some((username, password)) = self.sign_in_form(ui, host, r.x + 18.0, y, r.w - 36.0, "User name or email") {
                out.push(AccountAction::ForumLogin { host: host.to_string(), username, password });
            }
            y += body_h;
        }
        self.message(ui, host, r.x + 18.0, y + 12.0, r.w - 36.0, true);
    }

    /// Sites with no sign-in here, and why.
    fn others(&mut self, ui: &mut Ui, col: &mut Col, out: &mut Vec<AccountAction>) {
        let c = ui.c;
        let rows = [
            ("Royal Road", "Can't sign in here: its pages ask for a browser check, which Flax Reader doesn't do. Follow its stories by link under Following."),
            ("Standard Ebooks and your catalogues", "Patrons and catalogues with a sign-in (Calibre, Kavita, Komga) keep it with the catalogue, under Settings › Sources."),
            ("Project Gutenberg", "No account needed."),
        ];
        let w = col.w - 36.0;
        let hs: Vec<f32> = rows.iter().map(|(_, d)| ui.measure(d, &TextStyle::ui(13.0), TextOpts::wrap(w).line_height(1.35)).h).collect();
        let r = col.take(16.0 + hs.iter().map(|h| h + 30.0).sum::<f32>() + 46.0);
        col.gap(12.0);
        ui.card(r);
        let mut y = r.y + 14.0;
        for ((name, desc), h) in rows.iter().zip(&hs) {
            ui.text(name, &TextStyle::ui(14.5).semibold(), r.x + 18.0, y, c.text);
            let t = ui.measure(desc, &TextStyle::ui(13.0), TextOpts::wrap(w).line_height(1.35));
            ui.g.draw_text(t, r.x + 18.0, y + 22.0, c.muted);
            y += h + 30.0;
        }
        let bw = ui.button_width("Open Sources", Some(icons::GLOBE));
        if ui.button_ex(id("acct-sources"), Rect::new(r.x + 18.0, y + 2.0, bw, 34.0), "Open Sources", Some(icons::GLOBE), ButtonKind::Secondary, true) {
            out.push(AccountAction::GoSources);
        }

        let privacy = "Passwords go only to the site you sign in to, and are never saved. What's kept is each site's own sign-in cookie, in this app's settings file on this PC; signing out removes it.";
        let t = ui.measure(privacy, &TextStyle::ui(13.0), TextOpts::wrap(col.w - 36.0).line_height(1.45));
        let r = col.take(t.h + 54.0);
        ui.card(r);
        ui.text("Privacy", &TextStyle::ui(12.5).semibold(), r.x + 18.0, r.y + 14.0, c.muted);
        ui.g.draw_text(t, r.x + 18.0, r.y + 36.0, c.muted);
    }
}
