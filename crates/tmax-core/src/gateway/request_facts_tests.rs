use super::*;
use serde_json::{Value, json};

#[test]
fn strict_validation_matches_value_at_opaque_boundaries() {
    let deeply_nested = format!("{{\"extension\":{}{}}}", "[".repeat(130), "]".repeat(130));
    let cases: Vec<&[u8]> = vec![
        br#"{"extension":"\uD800"}"#,
        br#"{"extension":{"\uD800":true}}"#,
        br#"{"extension":["\uDC00"]}"#,
        br#"{"extension":"\uD834\uDD1E"}"#,
        br#"{"extension":{"$serde_json::private::Number":"bogus"}}"#,
        br#"{"extension":{"$serde_json::private::Number":"1","bad":"\uD800"}}"#,
        br#"{"extension":{"$serde_json::private::Number":"1e400"}}"#,
        b"{\"extension\":\"\xff\"}",
        b"{} trailing",
        deeply_nested.as_bytes(),
    ];
    for bytes in cases {
        assert_eq!(
            RequestFacts::parse(bytes).is_ok(),
            serde_json::from_slice::<Value>(bytes).is_ok(),
            "{}",
            String::from_utf8_lossy(bytes)
        );
    }
    for bytes in [
        b"null".as_slice(),
        b"[]",
        b"1e400",
        br#"{"$serde_json::private::Number":"1"}"#,
    ] {
        assert!(RequestFacts::parse(bytes).is_err());
    }
}

#[test]
fn duplicate_decoded_fields_replace_instead_of_accumulating() {
    let raw = br#"{
        "model":"first","mo\u0064el":"last",
        "messages":[{"role":"assistant","content":[{"type":"image"}]}],
        "messages":[{"role":"assistant","role":"user","content":[{"type":"tool_use"}],"content":"text"}],
        "input":[{"type":"item_reference"}],"input":"text",
        "tools":[{"type":"file_search","vector_store_ids":["remote"]}],"tools":[],
        "extension":{"key":1,"k\u0065y":2}
    }"#;
    let actual = RequestFacts::parse(raw).unwrap();
    let mut expected = RequestFacts::from_value(serde_json::from_slice(raw).unwrap()).unwrap();
    expected.normalization_required = true;
    assert_eq!(actual, expected);
    assert_eq!(actual.model.as_deref(), Some("last"));
    assert!(!actual.tool_use);
    assert!(!actual.remote_state);
    assert!(!actual.unbound_resources);
    assert!(actual.new_session("chat-completions"));
    assert_eq!(actual.input_modalities, BTreeSet::from(["text"]));
}

#[test]
fn presence_type_and_structured_format_precedence_are_preserved() {
    for body in [
        json!({"reasoning_effort":null,"reasoning":{"effort":"high"},"response_format":{"type":null},"text":{"format":{"type":"json_schema"}}}),
        json!({"reasoning_effort":42,"reasoning":false,"temperature":false,"max_tokens":"999"}),
        json!({"reasoning":{"effort":""},"response_format":null,"text":{"format":{"type":"json_schema"}}}),
    ] {
        let facts = RequestFacts::parse(&serde_json::to_vec(&body).unwrap()).unwrap();
        assert_eq!(
            facts.reasoning_effort.present(),
            body.get("reasoning_effort").is_some()
        );
        assert_eq!(
            facts.reasoning_effort.as_str(),
            body.get("reasoning_effort").and_then(Value::as_str)
        );
        assert_eq!(
            facts.nested_effort.as_str(),
            body.pointer("/reasoning/effort").and_then(Value::as_str)
        );
        assert_eq!(
            facts.structured,
            body.pointer("/response_format/type")
                .or_else(|| body.pointer("/text/format/type"))
                .and_then(Value::as_str)
                == Some("json_schema")
        );
        assert_eq!(
            facts.temperature,
            body.get("temperature").is_some_and(|v| !v.is_null())
        );
    }
}

#[test]
fn resource_and_modality_facts_only_follow_native_paths() {
    let facts = RequestFacts::from_value(json!({
        "messages":[{"role":"user","content":[
            {"type":"document","source":{"media_type":"application/pdf","file_id":false}},
            {"type":"input_audio","data":"opaque"},
            {"type":"tool_result"}
        ]}],
        "input":[{"type":"input_video"}],
        "modalities":["image",42,"text","image"],
        "extension":{"type":"image","file_id":"opaque"}
    }))
    .unwrap();
    assert!(facts.tool_use);
    assert!(facts.unbound_resources);
    assert_eq!(
        facts.input_modalities,
        BTreeSet::from(["audio", "pdf", "video"])
    );
    assert_eq!(
        facts.output_modalities,
        BTreeSet::from(["image".into(), "text".into()])
    );
    assert!(!facts.remote_state);
}

#[test]
fn response_state_false_null_and_empty_values_keep_their_distinctions() {
    for (value, remote, deepseek) in [
        (json!(null), false, false),
        (json!(false), true, false),
        (json!(""), true, true),
        (json!([]), true, true),
    ] {
        let facts = RequestFacts::from_value(
            json!({"previous_response_id":value,"input":[],"store":false}),
        )
        .unwrap();
        assert_eq!(facts.remote_state, remote);
        assert_eq!(facts.deepseek_state, deepseek);
        assert_eq!(facts.independent("responses"), !remote);
    }
    let input = RequestFacts::from_value(json!({"input":[{"type":"tool_use"}]})).unwrap();
    assert!(!input.tool_use);
}

#[test]
fn out_of_range_budgets_parse_like_any_value() {
    // Requested output budgets are forwarded, not compared locally; extreme
    // lexemes must still parse and agree between the two extractors.
    for value in [
        "1e400",
        "1e-400",
        "18446744073709551616",
        "1000.000000000000000001",
        "-1e400",
    ] {
        let body = format!("{{\"max_tokens\":{value}}}");
        let facts = RequestFacts::parse(body.as_bytes()).unwrap();
        let expected: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(facts, RequestFacts::from_value(expected).unwrap());
    }
}

#[test]
fn raw_value_token_remains_an_opaque_native_extension() {
    let facts =
        RequestFacts::parse(br#"{"extension":{"$serde_json::private::RawValue":"not JSON"}}"#)
            .unwrap();
    assert!(!facts.normalization_required);
    assert!(!facts.tool_use);
    assert!(!facts.remote_state);
}

#[test]
fn fixed_effort_bypasses_malformed_scalar_but_not_response_container() {
    let model = crate::config::Model::default();
    for (protocol, body) in [
        ("chat-completions", json!({"reasoning_effort":42})),
        ("responses", json!({"reasoning":{"effort":42}})),
    ] {
        let facts = RequestFacts::from_value(body).unwrap();
        assert_eq!(
            crate::reasoning::resolve(&model, protocol, &facts, None)
                .unwrap_err()
                .code,
            "reasoning_parameter_invalid"
        );
        assert_eq!(
            crate::reasoning::resolve(&model, protocol, &facts, Some("high")).unwrap(),
            Some("high")
        );
    }
    let malformed =
        RequestFacts::from_value(json!({"reasoning":42,"thinking":{"type":"disabled"}})).unwrap();
    assert_eq!(
        crate::reasoning::resolve(&model, "responses", &malformed, Some("high"))
            .unwrap_err()
            .code,
        "reasoning_parameter_invalid"
    );
    assert_eq!(
        crate::reasoning::request_mismatch("responses", &malformed, "high"),
        Some("reasoning_parameter_invalid")
    );
    let disabled = RequestFacts::from_value(json!({"thinking":{"type":"disabled"}})).unwrap();
    assert_eq!(
        crate::reasoning::request_mismatch("chat-completions", &disabled, "high"),
        Some("reasoning_toggle_conflict")
    );
    assert_eq!(
        crate::reasoning::request_mismatch("chat-completions", &disabled, "none"),
        None
    );
}
