use std::{error::Error, io, time::Duration};

use serde_json::json;
use synapse_action::{ACTION_QUEUE_CAPACITY, ActionHandle};
use synapse_core::{Action, Event, EventFilter, EventSource, error_codes};
use synapse_reflex::{
    EventBus, REFLEX_TICK_LATE_KIND, ReflexScheduler, ScheduledReflex, SchedulerConfig,
    SchedulerTrigger,
};

const WAIT_TIMEOUT: Duration = Duration::from_secs(3);

#[test]
fn zero_reflexes_tick_fires_without_dispatch() -> Result<(), Box<dyn Error>> {
    let bus = EventBus::default();
    let (action_handle, action_rx) = ActionHandle::channel();
    assert_eq!(action_rx.len(), 0);

    let mut scheduler = ReflexScheduler::spawn(
        bus,
        action_handle,
        Vec::new(),
        SchedulerConfig::default().with_max_ticks(24),
    )?;
    let samples = scheduler.wait_for_samples(24, WAIT_TIMEOUT);
    scheduler.stop()?;

    assert_eq!(samples.len(), 24);
    assert!(samples.iter().all(|sample| sample.dispatched_actions == 0));
    assert_eq!(action_rx.len(), 0);
    Ok(())
}

#[test]
fn on_event_reflex_pulls_bus_event_and_dispatches() -> Result<(), Box<dyn Error>> {
    let bus = EventBus::default();
    let (action_handle, action_rx) = ActionHandle::channel();
    let reflex = ScheduledReflex::on_event(
        "reflex-on-event",
        EventFilter::Kind {
            kind: "wanted".to_owned(),
        },
        vec![Action::ReleaseAll],
    );
    assert_eq!(action_rx.len(), 0);
    let mut scheduler = ReflexScheduler::spawn(
        bus.clone(),
        action_handle,
        vec![reflex],
        SchedulerConfig::default().with_max_ticks(8),
    )?;
    let _report = bus.publish(event(1, "wanted"));
    let samples = scheduler.wait_for_samples(8, WAIT_TIMEOUT);
    scheduler.stop()?;

    let pulled = samples
        .iter()
        .map(|sample| sample.pulled_events)
        .sum::<usize>();
    let dispatched = samples
        .iter()
        .map(|sample| sample.dispatched_actions)
        .sum::<usize>();

    assert!(pulled >= 1);
    assert_eq!(dispatched, 1);
    assert_eq!(action_rx.len(), 1);
    Ok(())
}

#[test]
fn thirty_two_reflexes_fire_same_tick_without_tick_late() -> Result<(), Box<dyn Error>> {
    let bus = EventBus::default();
    let late_events = bus.subscribe(
        EventFilter::Kind {
            kind: REFLEX_TICK_LATE_KIND.to_owned(),
        },
        Vec::new(),
        false,
    )?;
    let (action_handle, action_rx) = ActionHandle::channel();
    let reflexes = (0..32)
        .map(|index| {
            ScheduledReflex::every_tick(format!("reflex-{index:02}"), vec![Action::ReleaseAll])
        })
        .collect::<Vec<_>>();
    assert_eq!(reflexes.len(), 32);
    assert_eq!(action_rx.len(), 0);
    assert_eq!(late_events.len(), 0);

    let mut scheduler = ReflexScheduler::spawn(
        bus,
        action_handle,
        reflexes,
        SchedulerConfig::default().with_max_ticks(1),
    )?;
    let samples = scheduler.wait_for_samples(1, WAIT_TIMEOUT);
    scheduler.stop()?;

    let late = late_events.drain();
    let Some(sample) = samples.first().copied() else {
        return Err(Box::new(io::Error::other(
            "scheduler did not record the expected tick sample",
        )));
    };

    assert_eq!(sample.dispatched_actions, 32);
    assert!(!sample.late);
    assert_eq!(action_rx.len(), 32);
    assert!(late.is_empty());
    Ok(())
}

#[test]
fn blocked_dispatch_path_emits_reflex_tick_late() -> Result<(), Box<dyn Error>> {
    let bus = EventBus::default();
    let late_events = bus.subscribe(
        EventFilter::Kind {
            kind: REFLEX_TICK_LATE_KIND.to_owned(),
        },
        Vec::new(),
        false,
    )?;
    let (action_handle, action_rx) = ActionHandle::channel();
    for _ in 0..ACTION_QUEUE_CAPACITY {
        action_handle.try_execute(Action::ReleaseAll)?;
    }
    assert_eq!(action_rx.len(), ACTION_QUEUE_CAPACITY);
    assert_eq!(late_events.len(), 0);

    let reflex = ScheduledReflex::every_tick("reflex-blocked", vec![Action::ReleaseAll]);
    let mut scheduler = ReflexScheduler::spawn(
        bus,
        action_handle,
        vec![reflex],
        SchedulerConfig::default().with_max_ticks(1),
    )?;
    let samples = scheduler.wait_for_samples(1, WAIT_TIMEOUT);
    scheduler.stop()?;

    let late = late_events.drain();
    let Some(sample) = samples.first().copied() else {
        return Err(Box::new(io::Error::other(
            "scheduler did not record blocked-dispatch sample",
        )));
    };

    assert_eq!(action_rx.len(), ACTION_QUEUE_CAPACITY);
    assert_eq!(sample.dispatched_actions, 0);
    assert!(sample.late);
    assert_eq!(late.len(), 1);
    assert_eq!(late[0].data["code"], error_codes::REFLEX_TICK_LATE);
    assert_eq!(late[0].data["reason"], "dispatch_blocked");
    Ok(())
}

#[test]
fn scheduler_rejects_invalid_trigger_filter() {
    let bus = EventBus::default();
    let (action_handle, action_rx) = ActionHandle::channel();
    let reflex = ScheduledReflex {
        reflex_id: "reflex-invalid-filter".to_owned(),
        trigger: SchedulerTrigger::OnEvent(EventFilter::And { args: Vec::new() }),
        then: vec![Action::ReleaseAll],
        priority: 0,
    };
    assert_eq!(action_rx.len(), 0);

    let error = match ReflexScheduler::spawn(
        bus,
        action_handle,
        vec![reflex],
        SchedulerConfig::default(),
    ) {
        Ok(_scheduler) => panic!("invalid event filter must prevent scheduler spawn"),
        Err(error) => error,
    };

    assert_eq!(error.code(), error_codes::REFLEX_FILTER_INVALID);
    assert_eq!(action_rx.len(), 0);
}

fn event(seq: u64, kind: &str) -> Event {
    Event {
        seq,
        at: chrono::Utc::now(),
        source: EventSource::System,
        kind: kind.to_owned(),
        data: json!({ "seq": seq, "kind": kind }),
        correlations: Vec::new(),
    }
}
