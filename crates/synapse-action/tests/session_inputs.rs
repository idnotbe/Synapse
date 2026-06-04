use std::sync::Arc;

use synapse_action::{ActionBackend, ActionEmitter, RecordingBackend};
use synapse_core::{Action, Backend, GamepadController, GamepadReport, Key, KeyCode, PadButton};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn session_release_keeps_other_session_inputs_held() {
    let cancel = CancellationToken::new();
    let backend: Arc<dyn ActionBackend> = Arc::new(RecordingBackend::new());
    let (handle, snapshot_handle, join) =
        ActionEmitter::spawn_with_backend(cancel.clone(), backend);
    let session_a = handle.with_session_id(Some("session-a".to_owned()));
    let session_b = handle.with_session_id(Some("session-b".to_owned()));

    session_a
        .execute(Action::KeyDown {
            key: key("ctrl"),
            backend: Backend::Software,
        })
        .await
        .unwrap_or_else(|error| panic!("session A keydown should succeed: {error}"));
    session_b
        .execute(Action::KeyDown {
            key: key("shift"),
            backend: Backend::Software,
        })
        .await
        .unwrap_or_else(|error| panic!("session B keydown should succeed: {error}"));
    session_b
        .execute(Action::PadReport {
            pad: 2,
            report: held_pad_report(),
        })
        .await
        .unwrap_or_else(|error| panic!("session B pad report should succeed: {error}"));

    let before = snapshot_handle
        .snapshot()
        .await
        .unwrap_or_else(|error| panic!("snapshot before session release should succeed: {error}"));
    let before_ownership = handle
        .session_inputs_snapshot()
        .unwrap_or_else(|error| panic!("ownership before session release should read: {error}"));
    println!(
        "readback=session_inputs edge=distinct_sessions before_state={before:?} before_ownership={before_ownership:?}"
    );
    assert_eq!(before.held_keys.len(), 2);
    assert_eq!(before.pad_state.len(), 1);

    let summary_a = handle
        .release_session_inputs("session-a")
        .await
        .unwrap_or_else(|error| panic!("session A release should succeed: {error}"));
    let after_a = snapshot_handle
        .snapshot()
        .await
        .unwrap_or_else(|error| panic!("snapshot after session A release should succeed: {error}"));
    let after_a_ownership = handle
        .session_inputs_snapshot()
        .unwrap_or_else(|error| panic!("ownership after session A release should read: {error}"));
    println!(
        "readback=session_inputs edge=distinct_sessions after_a_state={after_a:?} after_a_ownership={after_a_ownership:?} summary={summary_a:?}"
    );
    assert_eq!(summary_a.released_keys, 1);
    assert_eq!(summary_a.neutralized_pads, 0);
    assert_eq!(after_a.held_keys, vec![key("shift")]);
    assert_eq!(after_a.pad_state.len(), 1);

    let summary_b = handle
        .release_session_inputs("session-b")
        .await
        .unwrap_or_else(|error| panic!("session B release should succeed: {error}"));
    let after_b = snapshot_handle
        .snapshot()
        .await
        .unwrap_or_else(|error| panic!("snapshot after session B release should succeed: {error}"));
    println!(
        "readback=session_inputs edge=distinct_sessions after_b_state={after_b:?} summary={summary_b:?}"
    );
    assert_eq!(summary_b.released_keys, 1);
    assert_eq!(summary_b.neutralized_pads, 1);
    assert!(after_b.held_keys.is_empty());
    assert!(after_b.pad_state.is_empty());

    cancel.cancel();
    let final_snapshot = join
        .await
        .unwrap_or_else(|error| panic!("emitter task should join: {error}"));
    assert!(final_snapshot.held_keys.is_empty());
    assert!(final_snapshot.pad_state.is_empty());
}

#[tokio::test]
async fn session_release_retains_shared_input_until_last_owner() {
    let cancel = CancellationToken::new();
    let backend: Arc<dyn ActionBackend> = Arc::new(RecordingBackend::new());
    let (handle, snapshot_handle, join) =
        ActionEmitter::spawn_with_backend(cancel.clone(), backend);
    let session_a = handle.with_session_id(Some("session-a".to_owned()));
    let session_b = handle.with_session_id(Some("session-b".to_owned()));

    for session in [&session_a, &session_b] {
        session
            .execute(Action::KeyDown {
                key: key("alt"),
                backend: Backend::Software,
            })
            .await
            .unwrap_or_else(|error| panic!("shared keydown should succeed: {error}"));
    }
    let before = snapshot_handle
        .snapshot()
        .await
        .unwrap_or_else(|error| panic!("snapshot before shared release should succeed: {error}"));
    println!("readback=session_inputs edge=shared_key before_state={before:?}");
    assert_eq!(before.held_keys, vec![key("alt")]);

    let summary_a = handle
        .release_session_inputs("session-a")
        .await
        .unwrap_or_else(|error| panic!("session A shared release should succeed: {error}"));
    let after_a = snapshot_handle
        .snapshot()
        .await
        .unwrap_or_else(|error| panic!("snapshot after shared release should succeed: {error}"));
    println!(
        "readback=session_inputs edge=shared_key after_a_state={after_a:?} summary={summary_a:?}"
    );
    assert_eq!(summary_a.released_keys, 0);
    assert_eq!(summary_a.retained_shared_inputs, 1);
    assert_eq!(after_a.held_keys, vec![key("alt")]);

    let summary_b = handle
        .release_session_inputs("session-b")
        .await
        .unwrap_or_else(|error| panic!("session B shared release should succeed: {error}"));
    let after_b = snapshot_handle.snapshot().await.unwrap_or_else(|error| {
        panic!("snapshot after final shared release should succeed: {error}")
    });
    println!(
        "readback=session_inputs edge=shared_key after_b_state={after_b:?} summary={summary_b:?}"
    );
    assert_eq!(summary_b.released_keys, 1);
    assert!(after_b.held_keys.is_empty());

    cancel.cancel();
    let final_snapshot = join
        .await
        .unwrap_or_else(|error| panic!("emitter task should join: {error}"));
    assert!(final_snapshot.held_keys.is_empty());
}

#[tokio::test]
async fn unknown_session_release_does_not_emit_global_release() {
    let cancel = CancellationToken::new();
    let backend: Arc<dyn ActionBackend> = Arc::new(RecordingBackend::new());
    let (handle, snapshot_handle, join) =
        ActionEmitter::spawn_with_backend(cancel.clone(), backend);
    let session_a = handle.with_session_id(Some("session-a".to_owned()));

    session_a
        .execute(Action::KeyDown {
            key: key("ctrl"),
            backend: Backend::Software,
        })
        .await
        .unwrap_or_else(|error| panic!("session A keydown should succeed: {error}"));
    let before = snapshot_handle
        .snapshot()
        .await
        .unwrap_or_else(|error| panic!("snapshot before unknown release should succeed: {error}"));

    let summary = handle
        .release_session_inputs("missing-session")
        .await
        .unwrap_or_else(|error| panic!("unknown session release should succeed as noop: {error}"));
    let after = snapshot_handle
        .snapshot()
        .await
        .unwrap_or_else(|error| panic!("snapshot after unknown release should succeed: {error}"));
    println!(
        "readback=session_inputs edge=unknown_session before_state={before:?} after_state={after:?} summary={summary:?}"
    );
    assert_eq!(summary.released_keys, 0);
    assert_eq!(summary.released_buttons, 0);
    assert_eq!(summary.neutralized_pads, 0);
    assert_eq!(before, after);

    handle
        .execute(Action::ReleaseAll)
        .await
        .unwrap_or_else(|error| panic!("release_all cleanup should succeed: {error}"));
    cancel.cancel();
    let final_snapshot = join
        .await
        .unwrap_or_else(|error| panic!("emitter task should join: {error}"));
    assert!(final_snapshot.held_keys.is_empty());
}

fn key(value: &str) -> Key {
    Key {
        code: KeyCode::Named {
            value: value.to_owned(),
        },
        use_scancode: false,
    }
}

fn held_pad_report() -> GamepadReport {
    GamepadReport {
        controller: GamepadController::X360,
        buttons: vec![PadButton::A],
        thumb_l: (0.0, 0.0),
        thumb_r: (0.0, 0.0),
        lt: 0.0,
        rt: 0.0,
    }
}
