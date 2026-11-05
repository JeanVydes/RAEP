use raep_core::{DeliveryError, IdempotencyCache, ReplayWindow, Sequencer};

#[test]
fn sequencer_is_monotonic() {
    let mut sequencer = Sequencer::default();
    assert_eq!(sequencer.peek(), 1);
    assert_eq!(sequencer.allocate(), 1);
    assert_eq!(sequencer.allocate(), 2);
}

#[test]
fn replay_window_reports_truncation_and_acknowledges() {
    let mut window = ReplayWindow::new(2).unwrap();
    window.append(1, "one").unwrap();
    window.append(2, "two").unwrap();
    window.append(3, "three").unwrap();

    assert_eq!(
        window.replay_after(0),
        Err(DeliveryError::ReplayTruncated {
            requested: 1,
            first: 2
        })
    );
    assert_eq!(
        window.replay_after(1).unwrap(),
        vec![(2, "two"), (3, "three")]
    );
    window.acknowledge(2);
    assert_eq!(window.first_sequence(), Some(3));
}

#[test]
fn replay_window_rejects_non_monotonic_events() {
    let mut window = ReplayWindow::new(2).unwrap();
    window.append(2, "two").unwrap();
    assert_eq!(
        window.append(2, "duplicate"),
        Err(DeliveryError::NonMonotonic {
            sequence: 2,
            last: 2
        })
    );
}

#[test]
fn idempotency_cache_is_bounded_and_replaces_values() {
    let mut cache = IdempotencyCache::new(2).unwrap();
    assert_eq!(cache.insert("a", 1), None);
    assert_eq!(cache.insert("b", 2), None);
    assert_eq!(cache.insert("a", 3), Some(1));
    assert_eq!(cache.get(&"a"), Some(&3));
    cache.insert("c", 4);
    assert_eq!(cache.get(&"b"), None);
    assert_eq!(cache.len(), 2);
}
