use std::sync::{Mutex, MutexGuard, PoisonError};

static LEASE_SERIAL: Mutex<()> = Mutex::new(());
static DAEMON_LIFECYCLE_SERIAL: Mutex<()> = Mutex::new(());

pub(crate) struct LeaseSerialGuard {
    _guard: MutexGuard<'static, ()>,
    reason: String,
}

impl Drop for LeaseSerialGuard {
    fn drop(&mut self) {
        let _prior = synapse_action::lease::force_clear(&self.reason);
    }
}

pub(crate) fn lease_serial(reason: &str) -> LeaseSerialGuard {
    let guard = LEASE_SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let _prior = synapse_action::lease::force_clear(reason);
    LeaseSerialGuard {
        _guard: guard,
        reason: reason.to_owned(),
    }
}

pub(crate) fn reset_lease(reason: &str) {
    let _prior = synapse_action::lease::force_clear(reason);
}

pub(crate) struct DaemonLifecycleSerialGuard {
    _guard: MutexGuard<'static, ()>,
}

impl Drop for DaemonLifecycleSerialGuard {
    fn drop(&mut self) {
        crate::daemon_lifecycle::reset_for_test();
    }
}

pub(crate) fn daemon_lifecycle_serial() -> DaemonLifecycleSerialGuard {
    let guard = DAEMON_LIFECYCLE_SERIAL
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    crate::daemon_lifecycle::reset_for_test();
    DaemonLifecycleSerialGuard { _guard: guard }
}
