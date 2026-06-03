use synapse_core::{PathPoint, PathSpec, VelocityProfile};

use crate::{ArcLengthPath, PathError};

const INVERSION_STEPS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimedPathPoint {
    pub elapsed_ms: f64,
    pub arclen: f64,
    pub point: PathPoint,
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum VelocityError {
    #[error(transparent)]
    Path(#[from] PathError),
    #[error("normalized time must be finite and within [0,1], got {t}")]
    InvalidTimeFraction { t: f64 },
    #[error("normalized position must be finite and within [0,1], got {position}")]
    InvalidPositionFraction { position: f64 },
    #[error("duration_ms must be finite and greater than zero, got {duration_ms}")]
    InvalidDuration { duration_ms: f64 },
    #[error("Fitts law parameter {field} is invalid: {value}")]
    InvalidFittsLawParameter { field: &'static str, value: f64 },
}

pub type VelocityResult<T> = Result<T, VelocityError>;

pub fn position_at_time(profile: VelocityProfile, t: f64) -> VelocityResult<f64> {
    validate_fraction_time(t)?;
    Ok(match profile {
        VelocityProfile::Constant | VelocityProfile::Linear => t,
        VelocityProfile::EaseInOut => smoothstep(t),
        VelocityProfile::MinimumJerk => minimum_jerk(t),
    })
}

pub fn normalized_velocity_at_time(profile: VelocityProfile, t: f64) -> VelocityResult<f64> {
    validate_fraction_time(t)?;
    Ok(match profile {
        VelocityProfile::Constant | VelocityProfile::Linear => 1.0,
        VelocityProfile::EaseInOut => 6.0 * t * (1.0 - t),
        VelocityProfile::MinimumJerk => 30.0 * t * t * (1.0 - t) * (1.0 - t),
    })
}

pub fn time_at_position(profile: VelocityProfile, position: f64) -> VelocityResult<f64> {
    validate_fraction_position(position)?;
    if position == 0.0 || position == 1.0 {
        return Ok(position);
    }

    match profile {
        VelocityProfile::Constant | VelocityProfile::Linear => Ok(position),
        VelocityProfile::EaseInOut | VelocityProfile::MinimumJerk => {
            invert_monotonic_profile(profile, position)
        }
    }
}

pub fn sample_timed_path(
    spec: &PathSpec,
    profile: VelocityProfile,
    samples: usize,
    duration_ms: f64,
) -> VelocityResult<Vec<TimedPathPoint>> {
    let arclen = ArcLengthPath::new(spec)?;
    sample_timed_arclen_path(&arclen, profile, samples, duration_ms)
}

pub fn sample_timed_arclen_path(
    path: &ArcLengthPath<'_>,
    profile: VelocityProfile,
    samples: usize,
    duration_ms: f64,
) -> VelocityResult<Vec<TimedPathPoint>> {
    validate_duration(duration_ms)?;
    if samples < 2 {
        return Err(PathError::InvalidSampleCount { samples }.into());
    }

    let last = samples - 1;
    let mut timed = Vec::with_capacity(samples);
    for index in 0..samples {
        let position_fraction = index as f64 / last as f64;
        let elapsed_fraction = time_at_position(profile, position_fraction)?;
        let arclen = path.length() * position_fraction;
        timed.push(TimedPathPoint {
            elapsed_ms: duration_ms * elapsed_fraction,
            arclen,
            point: path.point_at_arclen(arclen)?,
        });
    }
    Ok(timed)
}

pub fn fitts_law_duration_ms(
    distance_px: f64,
    target_width_px: f64,
    a_ms: f64,
    b_ms: f64,
) -> VelocityResult<f64> {
    validate_fitts("distance_px", distance_px, false)?;
    validate_fitts("target_width_px", target_width_px, true)?;
    validate_fitts("a_ms", a_ms, false)?;
    validate_fitts("b_ms", b_ms, true)?;

    Ok(a_ms + b_ms * (distance_px / target_width_px + 1.0).log2())
}

fn invert_monotonic_profile(profile: VelocityProfile, position: f64) -> VelocityResult<f64> {
    let mut low = 0.0;
    let mut high = 1.0;
    for _ in 0..INVERSION_STEPS {
        let mid = (low + high) * 0.5;
        let value = position_at_time(profile, mid)?;
        if value < position {
            low = mid;
        } else {
            high = mid;
        }
    }
    Ok((low + high) * 0.5)
}

fn validate_fraction_time(t: f64) -> VelocityResult<()> {
    if !t.is_finite() || !(0.0..=1.0).contains(&t) {
        return Err(VelocityError::InvalidTimeFraction { t });
    }
    Ok(())
}

fn validate_fraction_position(position: f64) -> VelocityResult<()> {
    if !position.is_finite() || !(0.0..=1.0).contains(&position) {
        return Err(VelocityError::InvalidPositionFraction { position });
    }
    Ok(())
}

fn validate_duration(duration_ms: f64) -> VelocityResult<()> {
    if !duration_ms.is_finite() || duration_ms <= 0.0 {
        return Err(VelocityError::InvalidDuration { duration_ms });
    }
    Ok(())
}

fn validate_fitts(field: &'static str, value: f64, strictly_positive: bool) -> VelocityResult<()> {
    if !value.is_finite()
        || if strictly_positive {
            value <= 0.0
        } else {
            value < 0.0
        }
    {
        return Err(VelocityError::InvalidFittsLawParameter { field, value });
    }
    Ok(())
}

fn smoothstep(t: f64) -> f64 {
    let t2 = t * t;
    (-2.0 * t).mul_add(t2, 3.0 * t2)
}

fn minimum_jerk(t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    let t4 = t3 * t;
    let t5 = t4 * t;
    10.0_f64.mul_add(t3, (-15.0_f64).mul_add(t4, 6.0 * t5))
}
