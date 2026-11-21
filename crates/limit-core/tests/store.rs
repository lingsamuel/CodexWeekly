use limit_core::{
    quota::{ParsedQuota, QuotaSample},
    settings::{Preferences, Settings, SourceKind},
    store::Store,
};

fn quota(account: &str, time: i64, remaining: f64, reset: i64) -> ParsedQuota {
    ParsedQuota {
        account_key: account.into(),
        personal_email_key: None,
        sample: QuotaSample {
            recorded_at: time,
            remaining_percent: remaining,
            resets_at: Some(reset),
            window_minutes: 10080,
        },
    }
}

#[test]
fn confirmed_personal_identity_shares_legacy_history_without_merging_conflicting_ids() {
    let store = Store::memory().unwrap();
    store.record(&quota("current-id", 90, 95.0, 1000)).unwrap();
    let mut wsl = quota("current-id", 100, 80.0, 1000);
    wsl.personal_email_key = Some("legacy-email".into());
    let windows = quota("legacy-email", 110, 75.0, 1000);
    store.record_for_source("wsl", &wsl, false).unwrap();
    store.record_for_source("windows", &windows, false).unwrap();
    store
        .record_for_source("windows", &quota("legacy-email", 100, 79.0, 1000), false)
        .unwrap();
    for account in ["current-id", "legacy-email"] {
        let points = store.history(account, 0, 200, 1200).unwrap();
        assert_eq!(points.len(), 3);
        assert_eq!(points[1].sample.remaining_percent, 80.0);
    }
    let mut different = quota("different-id", 130, 50.0, 1000);
    different.personal_email_key = Some("legacy-email".into());
    store.record_for_source("wsl", &different, false).unwrap();
    assert_eq!(store.history("current-id", 0, 200, 1200).unwrap().len(), 2);
    assert_eq!(
        store.history("different-id", 0, 200, 1200).unwrap().len(),
        1
    );
    assert_eq!(
        store.history("legacy-email", 0, 200, 1200).unwrap().len(),
        2
    );
}

#[test]
fn history_is_isolated_by_account_and_sorted() {
    let store = Store::memory().unwrap();
    store.record(&quota("a", 100, 80.0, 1000)).unwrap();
    store.record(&quota("b", 110, 50.0, 1000)).unwrap();
    store.record(&quota("a", 120, 75.0, 1000)).unwrap();
    let points = store.history("a", 90, 200, 1200).unwrap();
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].sample.remaining_percent, 80.0);
    assert_eq!(points[1].sample.recorded_at, 120);
    assert_ne!(
        points[0].segment, points[1].segment,
        "切回原账号后必须开始新段"
    );
    assert_eq!(store.latest().unwrap().unwrap().account_key, "a");
    assert_eq!(store.history("a", 115, 200, 1200).unwrap().len(), 1);
}

#[test]
fn sources_share_account_history_and_keep_their_own_last_readings() {
    let store = Store::memory().unwrap();
    store.record(&quota("a", 90, 95.0, 1000)).unwrap();
    assert!(store.latest_for_source("windows").unwrap().is_none());
    store
        .record_for_source("wsl:arch", &quota("a", 100, 80.0, 1000), false)
        .unwrap();
    store
        .record_for_source("windows", &quota("a", 100, 75.0, 1000), false)
        .unwrap();
    store
        .record_for_source("wsl:debian", &quota("a", 110, 74.0, 1000), false)
        .unwrap();
    store
        .record_for_source("wsl:debian", &quota("b", 120, 60.0, 1000), false)
        .unwrap();

    let arch = store.latest_for_source("wsl:arch").unwrap().unwrap();
    let windows = store.latest_for_source("windows").unwrap().unwrap();
    let debian = store.latest_for_source("wsl:debian").unwrap().unwrap();
    assert_eq!(arch.sample.remaining_percent, 80.0);
    assert_eq!(windows.sample.remaining_percent, 75.0);
    assert_eq!(debian.account_key, "b");
    assert_eq!(
        store
            .history(&windows.account_key, 0, 200, 1200)
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        store
            .history(&debian.account_key, 0, 200, 1200)
            .unwrap()
            .len(),
        1
    );

    store.prune(110).unwrap();
    assert!(store.latest_for_source("windows").unwrap().is_none());
    assert!(store.latest_for_source("wsl:debian").unwrap().is_some());
}

#[test]
fn reset_and_offline_gaps_split_chart_segments() {
    let store = Store::memory().unwrap();
    for q in [
        quota("a", 100, 80.0, 1000),
        quota("a", 160, 79.0, 1000),
        quota("a", 220, 100.0, 2000),
        quota("a", 2000, 98.0, 2000),
    ] {
        store.record(&q).unwrap();
    }
    let points = store.history("a", 0, 3000, 1200).unwrap();
    assert_eq!(points[0].segment, points[1].segment);
    assert_ne!(points[1].segment, points[2].segment);
    assert_ne!(points[2].segment, points[3].segment);
}

#[test]
fn prunes_old_records_and_bounds_chart_size() {
    let store = Store::memory().unwrap();
    for t in 1..=100 {
        store
            .record(&quota("a", t, 100.0 - (t as f64), 1000))
            .unwrap();
    }
    let points = store.history("a", 1, 100, 12).unwrap();
    assert!(points.len() <= 12);
    assert_eq!(points.first().unwrap().sample.recorded_at, 1);
    assert_eq!(points.last().unwrap().sample.recorded_at, 100);
    store.prune(50).unwrap();
    assert_eq!(
        store
            .history("a", 0, 100, 1200)
            .unwrap()
            .first()
            .unwrap()
            .sample
            .recorded_at,
        50
    );
}

#[test]
fn preferences_roundtrip_and_validate_input() {
    let store = Store::memory().unwrap();
    let mut prefs = Preferences::default();
    prefs.settings.distro = "Arch".into();
    prefs.locked = true;
    assert!(prefs.completion_sound);
    prefs.completion_sound = false;
    store.save_preferences(&prefs).unwrap();
    assert_eq!(store.preferences().unwrap(), prefs);
    for seconds in [0, 29, 3601] {
        assert!(Settings {
            poll_interval_secs: seconds,
            ..Settings::default()
        }
        .validate()
        .is_err());
    }
    assert!(Settings {
        distro: "--exec bad".into(),
        ..Settings::default()
    }
    .validate()
    .is_err());
    assert!(Settings {
        distro: "Arch\nother".into(),
        ..Settings::default()
    }
    .validate()
    .is_err());
}

#[test]
fn previous_settings_keep_their_wsl_source() {
    let settings: Settings =
        serde_json::from_str(r#"{"distro":"Arch","pollIntervalSecs":42}"#).unwrap();
    assert_eq!(settings.source, SourceKind::Wsl);
    assert_eq!(settings.distro, "Arch");
    assert_eq!(settings.poll_interval_secs, 42);
    let windows: Settings = serde_json::from_str(r#"{"source":"windows"}"#).unwrap();
    assert_eq!(windows.source, SourceKind::Windows);
}

#[test]
fn legacy_preferences_enable_completion_sound_by_default() {
    let prefs: Preferences =
        serde_json::from_str(r#"{"overlayVisible":false,"locked":true}"#).unwrap();
    assert!(prefs.completion_sound);
    assert!(!prefs.overlay_visible);
    assert!(prefs.locked);
}

#[test]
fn a_failed_poll_splits_the_next_success_even_before_the_gap_threshold() {
    let store = Store::memory().unwrap();
    store.record(&quota("a", 100, 80.0, 1000)).unwrap();
    store
        .record_with_boundary(&quota("a", 220, 79.0, 1000), true)
        .unwrap();
    let points = store.history("a", 0, 1000, 1200).unwrap();
    assert_ne!(points[0].segment, points[1].segment);
}

#[test]
fn editing_poll_interval_does_not_reinterpret_old_history() {
    let store = Store::memory().unwrap();
    store.record(&quota("a", 100, 80.0, 1000)).unwrap();
    store.record(&quota("a", 400, 79.0, 1000)).unwrap();
    let before = store.history("a", 0, 1000, 1200).unwrap();
    let mut preferences = Preferences::default();
    preferences.settings.poll_interval_secs = 3600;
    store.save_preferences(&preferences).unwrap();
    let after = store.history("a", 0, 1000, 1200).unwrap();
    assert_ne!(before[0].segment, before[1].segment);
    assert_eq!(before[1].segment, after[1].segment);
}
