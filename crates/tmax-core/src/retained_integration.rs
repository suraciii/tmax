use std::collections::BTreeMap;

// The only legacy regional product mapping. These IDs are accepted exclusively
// when decoding retained operational state, never as desired integrations.
const KIMI: &[(&str, &str)] = &[
    ("china", "kimi-code-plan-cn"),
    ("overseas", "kimi-code-plan-global"),
];
const MINIMAX: &[(&str, &str)] = &[
    ("china", "minimax-cn-coding-plan"),
    ("global", "minimax-coding-plan"),
];

pub(crate) fn regional_candidates(id: &str) -> Option<&'static [(&'static str, &'static str)]> {
    match id {
        "kimi-coding-plan" => Some(KIMI),
        "minimax-m-plan" => Some(MINIMAX),
        _ => None,
    }
}

// A former base selector may be a protocol subset, but every supplied base
// must match and the selection must identify exactly one qualified destination.
pub(crate) fn regional_selection(
    id: &str,
    region: Option<&str>,
    bases: &BTreeMap<String, String>,
) -> Option<&'static str> {
    let mut matches = regional_candidates(id)?.iter().filter(|(name, canonical)| {
        region.is_none_or(|selected| selected == *name)
            && crate::provider_integration::fixed_endpoints(canonical).is_ok_and(|fixed| {
                bases
                    .iter()
                    .all(|(protocol, base)| fixed.get(protocol) == Some(base))
            })
    });
    let selected = matches.next()?.1;
    if matches.next().is_some() {
        None
    } else {
        Some(selected)
    }
}
