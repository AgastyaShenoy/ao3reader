use crate::ao3::models::{UserSession, Work};
use crate::ao3::parser::Ao3Parser;
use std::time::Duration;
use ureq::http::Uri;
use ureq::{Agent, Cookie, ResponseExt};

const AO3_BASE_URL: &str = "https://archiveofourown.org";
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36 (AO3DesktopReader/1.0)";

/// Full-work pages of very long works run to several MB of HTML; ureq's default cap is 10 MB.
const MAX_BODY_BYTES: u64 = 64 * 1024 * 1024;

const SESSION_COOKIE: &str = "_otw_session";
const REMEMBER_COOKIE: &str = "_otw_remember_me";

/// Explains an unexpected AO3 response in reader terms.
fn unavailable_message(status: u16) -> String {
    match status {
        418 | 500..=599 => format!(
            "AO3 isn't answering requests right now (HTTP {status}). It may be down for maintenance; check archiveofourown.org in a browser and try again later."
        ),
        _ => format!("AO3 returned an unexpected response (HTTP {status})."),
    }
}

/// Blocking AO3 client. Call it from a worker thread, never from the UI thread.
/// Clones share one connection pool and cookie jar.
#[derive(Clone)]
pub struct Ao3Client {
    agent: Agent,
}

impl Ao3Client {
    pub fn new(session: Option<&UserSession>) -> Self {
        let agent: Agent = Agent::config_builder()
            .user_agent(DEFAULT_USER_AGENT)
            .max_redirects(10)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            .build()
            .into();
        let client = Self { agent };

        // Always set view_adult cookie so adult confirmation doesn't block requests
        client.set_cookie("view_adult", "true");

        // If session has cookies, restore them
        if let Some(sess) = session {
            if let Some(cookie_val) = &sess.session_cookie {
                client.restore_cookie(SESSION_COOKIE, cookie_val);
            }
            if let Some(remember) = &sess.remember_token {
                client.restore_cookie(REMEMBER_COOKIE, remember);
            }
        }

        client
    }

    /// Accepts either a bare cookie value or a full `name=value; ...` cookie string.
    fn restore_cookie(&self, name: &str, value: &str) {
        if value.is_empty() {
            return;
        }
        if value.contains('=') {
            self.add_cookie_str(value);
        } else {
            self.set_cookie(name, value);
        }
    }

    fn add_cookie_str(&self, cookie_str: &str) {
        let uri = Uri::from_static(AO3_BASE_URL);
        if let Ok(cookie) = Cookie::parse(cookie_str.to_string(), &uri) {
            let _ = self.agent.cookie_jar_lock().insert(cookie, &uri);
        }
    }

    /// Set session cookie manually (e.g. from user input)
    pub fn set_cookie(&self, name: &str, value: &str) {
        self.add_cookie_str(&format!("{name}={value}; Domain=.archiveofourown.org; Path=/"));
    }

    fn cookie_value(&self, name: &str) -> Option<String> {
        let jar = self.agent.cookie_jar_lock();
        let value = jar.iter().find(|c| c.name() == name).map(|c| c.value().to_string());
        value
    }

    /// GET a page and return (status, final URL, body).
    fn get_page(&self, url: &str) -> Result<(u16, String, String), ureq::Error> {
        let mut resp = self.agent.get(url).call()?;
        let status = resp.status().as_u16();
        let final_url = resp.get_uri().to_string();
        let body = resp.body_mut().with_config().limit(MAX_BODY_BYTES).read_to_string()?;
        Ok((status, final_url, body))
    }

    /// Login to AO3 with username and password
    pub fn login(&self, username: &str, password: &str) -> Result<UserSession, String> {
        let login_url = format!("{AO3_BASE_URL}/users/login");

        // 1. Fetch login page to get authenticity token
        let (_, _, html_text) = self
            .get_page(&login_url)
            .map_err(|e| format!("Failed to reach AO3 login page: {e}"))?;

        let token = {
            let doc = scraper::Html::parse_document(&html_text);
            Ao3Parser::extract_authenticity_token(&doc)
                .ok_or_else(|| "Could not locate CSRF authenticity token on login page".to_string())?
        };

        // 2. Submit credentials
        let params = [
            ("authenticity_token", token.as_str()),
            ("user[login]", username),
            ("user[password]", password),
            ("user[remember_me]", "1"),
            ("commit", "Log In"),
        ];

        let mut post_resp = self
            .agent
            .post(&login_url)
            .send_form(params)
            .map_err(|e| format!("Login request failed: {e}"))?;

        let final_url = post_resp.get_uri().to_string();
        let body = post_resp
            .body_mut()
            .with_config()
            .limit(MAX_BODY_BYTES)
            .read_to_string()
            .map_err(|e| format!("Failed to read login response: {e}"))?;

        if body.contains("The password or user name you entered doesn't match")
            || body.contains("Invalid user name or password")
            || body.contains("The password you entered is incorrect")
        {
            return Err("Invalid username or password. Please verify your AO3 credentials.".to_string());
        }

        // Check if greeting or user profile is accessible
        let logged_in = final_url.contains("/users/") || body.contains("Log Out") || body.contains(username);

        if logged_in {
            // Hand the session cookies back so they can be persisted and restored next launch.
            Ok(UserSession {
                username: Some(username.to_string()),
                session_cookie: self.cookie_value(SESSION_COOKIE),
                remember_token: self.cookie_value(REMEMBER_COOKIE),
                is_logged_in: true,
            })
        } else {
            Err("Login attempt did not complete successfully. AO3 may have presented a captcha or security verification.".to_string())
        }
    }

    /// Which AO3 user the current cookies belong to (None for a guest session).
    pub fn current_user(&self) -> Result<Option<String>, String> {
        let (status, _, body) = self.get_page(AO3_BASE_URL).map_err(|e| format!("Couldn't reach AO3: {e}"))?;
        if !(200..300).contains(&status) {
            return Err(unavailable_message(status));
        }
        let doc = scraper::Html::parse_document(&body);
        Ok(Ao3Parser::logged_in_user(&doc))
    }

    /// Fetch a full work by ID or URL
    pub fn fetch_work(&self, work_id_or_url: &str) -> Result<Work, String> {
        let work_id = Ao3Parser::extract_work_id(work_id_or_url)
            .ok_or_else(|| format!("Invalid AO3 Work ID or URL: '{work_id_or_url}'"))?;

        let full_url = format!("{AO3_BASE_URL}/works/{work_id}?view_adult=true&view_full_work=true");

        let (status, _, body) = self
            .get_page(&full_url)
            .map_err(|e| format!("Failed to download work from AO3: {e}"))?;

        if status == 404 {
            return Err(format!("Work #{work_id} was not found on AO3 (404). It might have been deleted."));
        } else if status == 429 {
            return Err("AO3 rate limit reached (HTTP 429). Please wait a few moments before trying again.".to_string());
        } else if !(200..300).contains(&status) {
            return Err(unavailable_message(status));
        }

        Ao3Parser::parse_work(&body, &work_id)
    }

    /// Leave kudos on an AO3 work
    pub fn leave_kudos(&self, work_id: &str) -> Result<String, String> {
        let work_url = format!("{AO3_BASE_URL}/works/{work_id}");
        let (_, _, body) = self
            .get_page(&work_url)
            .map_err(|e| format!("Failed to open work page for kudos: {e}"))?;

        let token = {
            let doc = scraper::Html::parse_document(&body);
            Ao3Parser::extract_authenticity_token(&doc).unwrap_or_default()
        };

        let kudos_url = format!("{AO3_BASE_URL}/kudos.js");
        let params = [
            ("authenticity_token", token.as_str()),
            ("kudo[commentable_id]", work_id),
            ("kudo[commentable_type]", "Work"),
        ];

        let mut kudos_resp = self
            .agent
            .post(&kudos_url)
            .header("X-Requested-With", "XMLHttpRequest")
            .send_form(params)
            .map_err(|e| format!("Failed to submit kudos: {e}"))?;

        let is_success = kudos_resp.status().is_success();
        let resp_text = kudos_resp.body_mut().read_to_string().unwrap_or_default();
        if resp_text.contains("You have already left kudos here") || resp_text.contains("already left kudos") {
            Ok("You have already left kudos here. :)".to_string())
        } else if resp_text.contains("Thank you for leaving kudos") || is_success {
            Ok("Thank you for leaving kudos! ❤️".to_string())
        } else {
            Ok("Kudos submitted!".to_string())
        }
    }

    /// Download EPUB from AO3 directly if available
    pub fn download_epub(&self, work_id: &str) -> Result<Vec<u8>, String> {
        let work_url = format!("{AO3_BASE_URL}/works/{work_id}");
        let (_, _, body) = self
            .get_page(&work_url)
            .map_err(|e| format!("Failed to fetch work page: {e}"))?;

        let href = {
            let doc = scraper::Html::parse_document(&body);
            let epub_sel = scraper::Selector::parse("li.download ul li a[href*='.epub']").unwrap();
            doc.select(&epub_sel)
                .next()
                .and_then(|a| a.value().attr("href"))
                .map(|h| {
                    if h.starts_with('/') {
                        format!("{AO3_BASE_URL}{h}")
                    } else {
                        h.to_string()
                    }
                })
                .unwrap_or_else(|| format!("{AO3_BASE_URL}/downloads/{work_id}/work.epub"))
        };

        let mut file_resp = self
            .agent
            .get(&href)
            .call()
            .map_err(|e| format!("Failed to download EPUB file: {e}"))?;

        let status = file_resp.status();
        if !status.is_success() {
            return Err(format!("EPUB download failed: AO3 returned status {}", status.as_u16()));
        }

        file_resp
            .body_mut()
            .with_config()
            .limit(MAX_BODY_BYTES)
            .read_to_vec()
            .map_err(|e| format!("Failed to read EPUB bytes: {e}"))
    }
}
