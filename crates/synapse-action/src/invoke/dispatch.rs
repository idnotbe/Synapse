use synapse_core::MouseButton;
#[cfg(any(test, windows))]
use synapse_core::{Action, AimCurve, AimNaturalParams, Backend, ButtonAction, MouseTarget};

#[cfg(test)]
use crate::ActionError;
use crate::{ActionBackend, ActionResult, EmitState};

use super::CoordinateFallbackPlan;
#[cfg(test)]
use super::ElementClickOutcome;

#[cfg(any(test, windows))]
pub(super) const FALLBACK_MOVE_DURATION_MS: u32 = 50;

#[cfg(test)]
pub(super) fn complete_click_attempt<B, F>(
    invoke_attempt: Result<(), InvokeAttemptError>,
    fallback_plan: F,
    backend: &B,
    state: &mut EmitState,
    button: MouseButton,
) -> ActionResult<ElementClickOutcome>
where
    B: ActionBackend,
    F: FnOnce() -> ActionResult<CoordinateFallbackPlan>,
{
    match invoke_attempt {
        Ok(()) => Ok(ElementClickOutcome::Invoked),
        Err(InvokeAttemptError::MissingPattern) => {
            let plan = fallback_plan()?;
            emit_coordinate_fallback_click(backend, state, button, plan)?;
            Ok(ElementClickOutcome::CoordinateFallback(plan))
        }
        Err(InvokeAttemptError::InvokeFailed(error)) => Err(error),
    }
}

#[cfg(test)]
pub(super) enum InvokeAttemptError {
    MissingPattern,
    InvokeFailed(ActionError),
}

#[cfg(any(test, windows))]
pub(super) fn emit_coordinate_fallback_click<B>(
    backend: &B,
    state: &mut EmitState,
    button: MouseButton,
    plan: CoordinateFallbackPlan,
) -> ActionResult<()>
where
    B: ActionBackend,
{
    backend.execute(
        &Action::MouseMove {
            to: MouseTarget::Screen {
                point: plan.screen_point,
            },
            curve: AimCurve::Natural {
                params: AimNaturalParams::FAST,
            },
            duration_ms: FALLBACK_MOVE_DURATION_MS,
            backend: Backend::Software,
        },
        state,
    )?;
    backend.execute(
        &Action::MouseButton {
            button,
            action: ButtonAction::Down,
            hold_ms: 0,
            backend: Backend::Software,
        },
        state,
    )?;
    backend.execute(
        &Action::MouseButton {
            button,
            action: ButtonAction::Up,
            hold_ms: 0,
            backend: Backend::Software,
        },
        state,
    )
}
