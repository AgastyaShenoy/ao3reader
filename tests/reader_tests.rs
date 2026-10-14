use ao3reader::ao3::models::*;
use ao3reader::ao3::parser::Ao3Parser;
use ao3reader::reader::theme::ReaderThemeType;
use ao3reader::storage::export::WorkExporter;

const SAMPLE_AO3_HTML: &str = r#"
<!DOCTYPE html>
<html>
<head><title>Test Fic - AuthorName - Archive of Our Own</title></head>
<body>
<div id="workskin">
    <div class="preface group">
        <h2 class="title heading">A Starlit Journey</h2>
        <h3 class="byline heading"><a rel="author" href="/users/Stargazer">Stargazer</a></h3>
        <div class="summary">
            <blockquote class="userstuff">
                <p>When the starship broke down in deep space, they only had each other.</p>
            </blockquote>
        </div>
        <div class="notes">
            <blockquote class="userstuff">
                <p>Written for the SciFi prompt week. Enjoy!</p>
            </blockquote>
        </div>
    </div>
    <dl class="work meta group">
        <dd class="rating tags"><ul><li><a href="/tags/Teen">Teen And Up Audiences</a></li></ul></dd>
        <dd class="warning tags"><ul><li><a href="/tags/NoWarnings">No Archive Warnings Apply</a></li></ul></dd>
        <dd class="category tags"><ul><li><a href="/tags/MM">M/M</a></li></ul></dd>
        <dd class="fandom tags"><ul><li><a href="/tags/StarTrek">Star Trek</a></li></ul></dd>
        <dd class="relationship tags"><ul><li><a href="/tags/KirkSpock">James T. Kirk/Spock</a></li></ul></dd>
        <dd class="character tags"><ul><li><a href="/tags/Kirk">James T. Kirk</a></li><li><a href="/tags/Spock">Spock</a></li></ul></dd>
        <dd class="freeform tags"><ul><li><a href="/tags/HurtComfort">Hurt/Comfort</a></li><li><a href="/tags/Space">Space Exploration</a></li></ul></dd>
        <dd class="language">English</dd>
        <dd class="stats">
            <dl class="stats">
                <dd class="published">2024-01-15</dd>
                <dd class="words">3,450</dd>
                <dd class="chapters">2/2</dd>
                <dd class="comments">42</dd>
                <dd class="kudos">1,250</dd>
                <dd class="bookmarks">310</dd>
                <dd class="hits">8,920</dd>
            </dl>
        </dd>
    </dl>
    <div id="chapters">
        <div class="chapter">
            <div class="chapter preface group">
                <h3 class="title">Chapter 1: The Distress Call</h3>
                <div class="summary"><blockquote class="userstuff"><p>Trouble begins.</p></blockquote></div>
                <div class="notes"><blockquote class="userstuff"><p>Thanks to my beta reader!</p></blockquote></div>
            </div>
            <div role="article">
                <div class="userstuff">
                    <p>The alarm klaxons blared through the bridge of the USS Enterprise.</p>
                    <blockquote>Captain, we have lost auxiliary power.</blockquote>
                    <p>Kirk gripped the armrests of his command chair firmly.</p>
                </div>
            </div>
            <div class="end notes">
                <blockquote class="userstuff"><p>Chapter 2 coming soon.</p></blockquote>
            </div>
        </div>
        <div class="chapter">
            <div class="chapter preface group">
                <h3 class="title">Chapter 2: Safe Harbor</h3>
            </div>
            <div role="article">
                <div class="userstuff">
                    <p>Light finally broke through the cosmic nebula.</p>
                    <hr>
                    <p>They had made it home together.</p>
                </div>
            </div>
        </div>
    </div>
</div>
</body>
</html>
"#;

#[test]
fn test_ao3_full_work_parsing() {
    let work = Ao3Parser::parse_work(SAMPLE_AO3_HTML, "999999").expect("Failed to parse sample work");

    assert_eq!(work.meta.id, "999999");
    assert_eq!(work.meta.title, "A Starlit Journey");
    assert_eq!(work.meta.authors, vec!["Stargazer".to_string()]);
    assert_eq!(work.meta.rating, WorkRating::Teen);
    assert_eq!(work.meta.warnings, vec!["No Archive Warnings Apply".to_string()]);
    assert_eq!(work.meta.fandoms, vec!["Star Trek".to_string()]);
    assert_eq!(work.meta.relationships, vec!["James T. Kirk/Spock".to_string()]);
    assert_eq!(work.meta.stats.words, 3450);
    assert_eq!(work.meta.stats.kudos, 1250);
    assert!(work.meta.is_complete);

    // Verify Chapters
    assert_eq!(work.chapters.len(), 2);

    let ch1 = &work.chapters[0];
    assert_eq!(ch1.title, "Chapter 1: The Distress Call");
    assert_eq!(ch1.summary.as_deref(), Some("Trouble begins."));
    assert_eq!(ch1.pre_notes.as_deref(), Some("Thanks to my beta reader!"));
    assert_eq!(ch1.post_notes.as_deref(), Some("Chapter 2 coming soon."));
    assert_eq!(ch1.paragraphs.len(), 3);
    assert!(ch1.paragraphs[0].contains("alarm klaxons"));

    let ch2 = &work.chapters[1];
    assert_eq!(ch2.title, "Chapter 2: Safe Harbor");
    assert_eq!(ch2.paragraphs.len(), 3);
    assert_eq!(ch2.paragraphs[1], "---");
    assert!(ch2.paragraphs[2].contains("made it home"));
}

#[test]
fn test_exporter_markdown() {
    let work = Ao3Parser::parse_work(SAMPLE_AO3_HTML, "999999").unwrap();
    let md = WorkExporter::to_markdown(&work);

    assert!(md.contains("# A Starlit Journey"));
    assert!(md.contains("**Author:** Stargazer"));
    assert!(md.contains("## Chapter 1: The Distress Call"));
    assert!(md.contains("alarm klaxons blared"));
}

#[test]
fn test_exporter_html() {
    let work = Ao3Parser::parse_work(SAMPLE_AO3_HTML, "999999").unwrap();
    let html = WorkExporter::to_html(&work);

    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("<h1>A Starlit Journey</h1>"));
    assert!(html.contains("<h2>Chapter 1: The Distress Call</h2>"));
    assert!(html.contains("<style>"));
}

#[test]
fn test_exporter_txt() {
    let work = Ao3Parser::parse_work(SAMPLE_AO3_HTML, "999999").unwrap();
    let txt = WorkExporter::to_txt(&work);

    assert!(txt.contains("A Starlit Journey"));
    assert!(txt.contains("By Stargazer"));
    assert!(txt.contains("--- Chapter 1: The Distress Call ---"));
}

#[test]
fn test_reader_themes() {
    for theme in ReaderThemeType::all() {
        let colors = theme.colors();
        assert!(!theme.display_name().is_empty());
        // Verify background and text have distinct components
        assert_ne!(colors.bg, colors.text);
    }
}

#[test]
fn test_restricted_work_detection() {
    let restricted_html = r#"
    <!DOCTYPE html>
    <html><body>
        <div class="flash error">
            <p>This work is only available to registered users of the Archive of Our Own. Please log in.</p>
        </div>
        <div id="signin" class="login"></div>
    </body></html>
    "#;

    let res = Ao3Parser::parse_work(restricted_html, "12345");
    assert!(res.is_err());
    let err_msg = res.unwrap_err();
    assert!(err_msg.contains("restricted to registered AO3 users"));
}

#[test]
fn test_ratings_and_categories() {
    assert_eq!(WorkRating::from_str_loose("Explicit"), WorkRating::Explicit);
    assert_eq!(WorkRating::from_str_loose("Mature"), WorkRating::Mature);
    assert_eq!(WorkRating::from_str_loose("Teen And Up"), WorkRating::Teen);
    assert_eq!(WorkRating::from_str_loose("General Audiences"), WorkRating::General);
    assert_eq!(WorkRating::from_str_loose("Not Rated"), WorkRating::NotRated);

    assert_eq!(Category::from_str_loose("M/M"), Category::MM);
    assert_eq!(Category::from_str_loose("F/M"), Category::FM);
    assert_eq!(Category::from_str_loose("F/F"), Category::FF);
    assert_eq!(Category::from_str_loose("Gen"), Category::Gen);
}

#[test]
fn test_in_reader_search() {
    use ao3reader::reader::search::ReaderSearchState;

    let work = Ao3Parser::parse_work(SAMPLE_AO3_HTML, "999999").unwrap();
    let mut search = ReaderSearchState::new();
    search.query = "power".to_string();
    search.search_all_chapters = true;
    search.execute_search(&work, 1);

    assert_eq!(search.matches.len(), 1);
    assert_eq!(search.matches[0].chapter_index, 1);
    assert!(search.matches[0].match_text.contains("auxiliary power"));
}

