use crate::ao3::models::{UserSession, Work};
use crate::ao3::parser::Ao3Parser;
use reqwest::cookie::Jar;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use std::sync::Arc;
use url::Url;

const AO3_BASE_URL: &str = "https://archiveofourown.org";
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36 (AO3DesktopReader/1.0)";

#[derive(Clone)]
pub struct Ao3Client {
    client: reqwest::Client,
    cookie_jar: Arc<Jar>,
}

impl Ao3Client {
    pub fn new(session: Option<&UserSession>) -> Self {
        let cookie_jar = Arc::new(Jar::default());
        let ao3_url = Url::parse(AO3_BASE_URL).unwrap();

        // Always set view_adult cookie so adult confirmation doesn't block requests
        cookie_jar.add_cookie_str("view_adult=true; Domain=.archiveofourown.org; Path=/", &ao3_url);

        // If session has cookies, restore them
        if let Some(sess) = session {
            if let Some(cookie_val) = &sess.session_cookie {
                if !cookie_val.is_empty() {
                    let formatted = if cookie_val.contains('=') {
                        cookie_val.clone()
                    } else {
                        format!("_otw_session={}; Domain=.archiveofourown.org; Path=/", cookie_val)
                    };
                    cookie_jar.add_cookie_str(&formatted, &ao3_url);
                }
            }
            if let Some(remember) = &sess.remember_token {
                if !remember.is_empty() {
                    let formatted = if remember.contains('=') {
                        remember.clone()
                    } else {
                        format!("_otw_remember_me={}; Domain=.archiveofourown.org; Path=/", remember)
                    };
                    cookie_jar.add_cookie_str(&formatted, &ao3_url);
                }
            }
        }

        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(DEFAULT_USER_AGENT));

        let client = reqwest::Client::builder()
            .cookie_provider(cookie_jar.clone())
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self { client, cookie_jar }
    }

    /// Set session cookie manually (e.g. from user input)
    pub fn set_cookie(&self, name: &str, value: &str) {
        let ao3_url = Url::parse(AO3_BASE_URL).unwrap();
        let cookie_str = format!("{name}={value}; Domain=.archiveofourown.org; Path=/");
        self.cookie_jar.add_cookie_str(&cookie_str, &ao3_url);
    }

    /// Login to AO3 with username and password
    pub async fn login(&self, username: &str, password: &str) -> Result<UserSession, String> {
        let login_url = format!("{AO3_BASE_URL}/users/login");

        // 1. Fetch login page to get authenticity token
        let resp = self
            .client
            .get(&login_url)
            .send()
            .await
            .map_err(|e| format!("Failed to reach AO3 login page: {e}"))?;

        let html_text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read AO3 login page: {e}"))?;

        // Scope Html doc so it drops before next await
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

        let post_resp = self
            .client
            .post(&login_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("Login request failed: {e}"))?;

        let final_url = post_resp.url().to_string();
        let body = post_resp
            .text()
            .await
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
            Ok(UserSession {
                username: Some(username.to_string()),
                session_cookie: None,
                remember_token: None,
                is_logged_in: true,
            })
        } else {
            Err("Login attempt did not complete successfully. AO3 may have presented a captcha or security verification.".to_string())
        }
    }

    /// Fetch a full work by ID or URL
    pub async fn fetch_work(&self, work_id_or_url: &str) -> Result<Work, String> {
        let work_id = Ao3Parser::extract_work_id(work_id_or_url)
            .ok_or_else(|| format!("Invalid AO3 Work ID or URL: '{work_id_or_url}'"))?;

        let full_url = format!("{AO3_BASE_URL}/works/{work_id}?view_adult=true&view_full_work=true");

        let response = self
            .client
            .get(&full_url)
            .send()
            .await
            .map_err(|e| format!("Failed to download work from AO3: {e}"))?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(format!("Work #{work_id} was not found on AO3 (404). It might have been deleted."));
        } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err("AO3 rate limit reached (HTTP 429). Please wait a few moments before trying again.".to_string());
        } else if !status.is_success() {
            return Err(format!("AO3 server returned status {status}"));
        }

        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {e}"))?;

        Ao3Parser::parse_work(&body, &work_id)
    }

    /// Leave kudos on an AO3 work
    pub async fn leave_kudos(&self, work_id: &str) -> Result<String, String> {
        let work_url = format!("{AO3_BASE_URL}/works/{work_id}");
        let page_resp = self
            .client
            .get(&work_url)
            .send()
            .await
            .map_err(|e| format!("Failed to open work page for kudos: {e}"))?;

        let body = page_resp
            .text()
            .await
            .map_err(|e| format!("Failed to read page: {e}"))?;

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

        let kudos_resp = self
            .client
            .post(&kudos_url)
            .form(&params)
            .header("X-Requested-With", "XMLHttpRequest")
            .send()
            .await
            .map_err(|e| format!("Failed to submit kudos: {e}"))?;

        let status = kudos_resp.status();
        let resp_text = kudos_resp.text().await.unwrap_or_default();
        if resp_text.contains("You have already left kudos here") || resp_text.contains("already left kudos") {
            Ok("You have already left kudos here. :)".to_string())
        } else if resp_text.contains("Thank you for leaving kudos") || status.is_success() {
            Ok("Thank you for leaving kudos! ❤️".to_string())
        } else {
            Ok("Kudos submitted!".to_string())
        }
    }

    /// Download EPUB from AO3 directly if available
    pub async fn download_epub(&self, work_id: &str) -> Result<Vec<u8>, String> {
        let work_url = format!("{AO3_BASE_URL}/works/{work_id}");
        let resp = self
            .client
            .get(&work_url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch work page: {e}"))?;

        let body = resp.text().await.map_err(|e| format!("Failed to read page: {e}"))?;

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

        let file_resp = self
            .client
            .get(&href)
            .send()
            .await
            .map_err(|e| format!("Failed to download EPUB file: {e}"))?;

        let bytes = file_resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to read EPUB bytes: {e}"))?;

        Ok(bytes.to_vec())
    }
}
