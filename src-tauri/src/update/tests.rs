use super::*;
use semver::Version;

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

#[test]
fn stable_only_goes_up() {
    assert!(is_update("stable", &v("0.1.0"), &v("0.2.0")));
    assert!(!is_update("stable", &v("0.2.0"), &v("0.1.0")));
    assert!(!is_update("stable", &v("0.2.0"), &v("0.2.0")));
}

#[test]
fn nightly_to_stable_can_go_down() {
    assert!(is_update(
        "stable",
        &v("0.2.1-nightly.20260928"),
        &v("0.2.0")
    ));
    assert!(!is_update("stable", &v("0.2.0"), &v("0.2.0")));
}

#[test]
fn nightly_ordering() {
    assert!(is_update(
        "nightly",
        &v("0.2.1-nightly.20260928"),
        &v("0.2.1-nightly.20260929")
    ));
    assert!(is_update(
        "nightly",
        &v("0.2.0"),
        &v("0.2.1-nightly.20260929")
    ));
    assert!(!is_update(
        "nightly",
        &v("0.2.1-nightly.20260929"),
        &v("0.2.1-nightly.20260928")
    ));
}
