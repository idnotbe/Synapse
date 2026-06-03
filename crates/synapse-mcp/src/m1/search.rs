use synapse_core::{AccessibleNode, DetectedEntity};

use crate::m1::{FindParams, FindResult, FindResultKind};

pub fn element_match(node: &AccessibleNode, params: &FindParams) -> Option<FindResult> {
    if params.in_window.is_some() && params.in_window.as_ref() != node.parent.as_ref() {
        return None;
    }
    if let Some(role) = &params.role
        && !node.role.eq_ignore_ascii_case(role)
    {
        return None;
    }
    if let Some(name_substring) = &params.name_substring
        && !contains_ascii_case(&node.name, name_substring)
    {
        return None;
    }
    if let Some(automation_id) = &params.automation_id
        && node.automation_id.as_deref() != Some(automation_id.as_str())
    {
        return None;
    }
    let mut score = 0.25;
    if let Some(query) = &params.query {
        if contains_ascii_case(&node.name, query)
            || contains_ascii_case(&node.role, query)
            || node
                .automation_id
                .as_deref()
                .is_some_and(|value| contains_ascii_case(value, query))
        {
            score += 0.65;
        } else if params.role.is_none()
            && params.name_substring.is_none()
            && params.automation_id.is_none()
        {
            return None;
        }
    }
    if node.focused {
        score += 0.1;
    }
    if synapse_a11y::cdp_backend_from_element_id(&node.element_id).is_some() {
        score += 0.05;
    }
    Some(FindResult {
        kind: FindResultKind::Element,
        element_id: Some(node.element_id.clone()),
        entity_id: None,
        name: Some(node.name.clone()),
        role: Some(node.role.clone()),
        automation_id: node.automation_id.clone(),
        class_label: None,
        bbox: node.bbox,
        score,
    })
}

pub fn entity_match(entity: &DetectedEntity, params: &FindParams) -> Option<FindResult> {
    let query = params.query.as_ref()?;
    contains_ascii_case(&entity.class_label, query).then_some(FindResult {
        kind: FindResultKind::Entity,
        element_id: None,
        entity_id: Some(entity.entity_id.clone()),
        name: None,
        role: None,
        automation_id: None,
        class_label: Some(entity.class_label.clone()),
        bbox: entity.bbox,
        score: entity.confidence,
    })
}

fn contains_ascii_case(value: &str, needle: &str) -> bool {
    value
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use synapse_core::{AccessibleNode, Rect, element_id};

    use super::*;

    fn node(element_id: synapse_core::ElementId, automation_id: &str) -> AccessibleNode {
        AccessibleNode {
            element_id,
            parent: None,
            name: "Apply".to_owned(),
            role: "button".to_owned(),
            automation_id: Some(automation_id.to_owned()),
            value: None,
            bbox: Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            },
            enabled: true,
            focused: false,
            patterns: Vec::new(),
            children_count: 0,
            depth: 0,
        }
    }

    #[test]
    fn cdp_duplicate_scores_above_uia_duplicate_for_actionable_find_result() {
        let params = FindParams {
            role: Some("button".to_owned()),
            name_substring: Some("Apply".to_owned()),
            ..FindParams::default()
        };
        let uia = node(element_id(0x100, "0000002a00000008"), "apply");
        let cdp = node(
            synapse_a11y::cdp_element_id(0x100, 8),
            "cdp:backendNodeId=8",
        );

        let uia_score = element_match(&uia, &params).expect("uia result").score;
        let cdp_score = element_match(&cdp, &params).expect("cdp result").score;

        println!(
            "readback=find_score edge=cdp_duplicate before=uia:{uia_score} after=cdp:{cdp_score}"
        );
        assert!(
            cdp_score > uia_score,
            "find should prefer actionable CDP web nodes over UIA duplicates"
        );
    }
}
