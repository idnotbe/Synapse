use serde_json::json;
use synapse_core::{PathPoint, PathSpec};

#[test]
fn path_spec_json_round_trips_and_defaults_catmull_alpha() -> Result<(), Box<dyn std::error::Error>>
{
    let input = json!({
        "kind": "catmull_rom",
        "waypoints": [
            {"x": 0.0, "y": 0.0},
            {"x": 10.0, "y": 0.0},
            {"x": 10.0, "y": 10.0},
            {"x": 20.0, "y": 10.0}
        ],
        "tension": 0.0,
        "closed": false
    });
    let parsed = serde_json::from_value::<PathSpec>(input.clone())?;
    let serialized = serde_json::to_value(&parsed)?;
    println!("readback=path_types edge=catmull_default_alpha before={input} after={serialized}");

    match parsed {
        PathSpec::CatmullRom { alpha, .. } => assert_eq!(alpha.to_bits(), 0.5_f64.to_bits()),
        other => panic!("expected catmull_rom path spec, got {other:?}"),
    }

    let closed_circle = PathSpec::Circle {
        center: PathPoint::new(5.0, -5.0),
        radius: 20.0,
    };
    let round_trip = serde_json::from_value::<PathSpec>(serde_json::to_value(&closed_circle)?)?;
    println!("readback=path_types edge=circle_round_trip after={round_trip:?}");
    assert_eq!(round_trip, closed_circle);

    let unknown_field = json!({
        "kind": "line",
        "from": {"x": 0.0, "y": 0.0},
        "to": {"x": 1.0, "y": 1.0},
        "extra": true
    });
    assert!(serde_json::from_value::<PathSpec>(unknown_field.clone()).is_err());
    println!("readback=path_types edge=unknown_field before={unknown_field} after=rejected");

    Ok(())
}
