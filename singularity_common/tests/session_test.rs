use singularity_common::sap::session::{SavedWl, title_without_url};

fn with_title(title: &str) -> SavedWl {
    SavedWl {
        argv: Vec::new(),
        cwd: None,
        app_id: None,
        title: title.to_string(),
    }
}

#[test]
fn url_from_title() {
    assert_eq!(
        with_title("A | B | https://example.com/?q=a").url(),
        Some("https://example.com/?q=a")
    );
    assert_eq!(
        with_title("https://example.com/a | A | B — Mozilla Firefox").url(),
        Some("https://example.com/a")
    );
    assert_eq!(with_title("bash | ~/code").url(), None);
    assert_eq!(with_title("https://example.com").url(), None);
}

#[test]
fn title_shown_without_url() {
    assert_eq!(
        title_without_url("https://example.com/a | A | B — Mozilla Firefox"),
        "A | B — Mozilla Firefox"
    );
    assert_eq!(title_without_url("A | https://example.com"), "A");
    assert_eq!(
        title_without_url("about:addons | Add-ons Manager"),
        "Add-ons Manager"
    );
    assert_eq!(title_without_url("bash | ~/code"), "bash | ~/code");
}

#[test]
fn web_app_from_app_id() {
    let with_app_id = |app_id: &str| SavedWl {
        app_id: Some(app_id.to_string()),
        ..with_title("")
    };
    assert_eq!(
        with_app_id("chrome-agimnkijcaahngcdmfeangaknmldooml-Profile_1").web_app(),
        Some(("agimnkijcaahngcdmfeangaknmldooml", "Profile 1".to_string()))
    );
    assert_eq!(with_app_id("chromium-browser").web_app(), None);
    assert_eq!(with_app_id("google-chrome").web_app(), None);
}
