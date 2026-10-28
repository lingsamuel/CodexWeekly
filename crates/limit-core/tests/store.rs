use limit_core::{
    quota::{ParsedQuota, QuotaSample},
    settings::{Preferences, Settings},
    store::Store,
};

fn quota(account: &str, time: i64, remaining: f64, reset: i64) -> ParsedQuota {
    ParsedQuota {
        account_key: account.into(),
        sample: QuotaSample {
            recorded_at: time,
            remaining_percent: remaining,
            resets_at: Some(reset),
            window_minutes: 10080,
        },
    }
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
    store.save_preferences(&prefs).unwrap();
    assert_eq!(store.preferences().unwrap(), prefs);
    for seconds in [0, 29, 3601] {
        assert!(Settings {
            distro: String::new(),
            poll_interval_secs: seconds
        }
        .validate()
        .is_err());
    }
    assert!(Settings {
        distro: "--exec bad".into(),
        poll_interval_secs: 60
    }
    .validate()
    .is_err());
    assert!(Settings {
        distro: "Arch\nother".into(),
        poll_interval_secs: 60
    }
    .validate()
    .is_err());
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
