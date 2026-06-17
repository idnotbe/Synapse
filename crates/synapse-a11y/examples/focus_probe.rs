//! Keystone probe for the #1000/#717 background-field-set fix.
//!
//! Calls `synapse_a11y::focus_element` (a background UIA `SetFocus`, no
//! foreground activation) on a single element id and prints the structured
//! outcome. Used to prove — against a real, non-foreground Chromium tab — that
//! UIA focus moves Chrome's DOM focus so the bridge can then type the right
//! field. This deliberately does NOT mutate any field value.
//!
//! Usage: `cargo run -p synapse-a11y --example focus_probe -- <element_id>`

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: focus_probe <element_id>");
        std::process::exit(2);
    });
    let id = match synapse_core::ElementId::parse(&arg) {
        Ok(id) => id,
        Err(err) => {
            println!("PROBE_RESULT status=parse_error element_id={arg} detail={err}");
            std::process::exit(2);
        }
    };
    match synapse_a11y::focus_element(&id) {
        Ok(()) => println!("PROBE_RESULT status=focus_ok element_id={arg}"),
        Err(err) => println!(
            "PROBE_RESULT status=focus_err element_id={arg} code={} detail={err}",
            err.code()
        ),
    }
}
