use super::*;
use serde_json::{Value, json};

fn text(bytes: &Bytes) -> &str {
    std::str::from_utf8(bytes).unwrap()
}

#[test]
fn unchanged_effective_values_preserve_wire_content_for_all_protocols() {
    for (protocol, body, effort) in [
        (
            "chat-completions",
            r#" {"model":"up\u0073tream","reasoning_effort":"h\u0069gh","input":"large"} "#,
            Some("high"),
        ),
        (
            "responses",
            r#" {"model":"upstream","reasoning":{"effort":"high","extension":false}} "#,
            Some("high"),
        ),
        (
            "messages",
            r#" {"model":"upstream","thinking":{"budget_tokens":1000}} "#,
            Some("high"),
        ),
        ("chat-completions", r#" {"model":"upstream"} "#, None),
        ("responses", r#" {"model":"upstream"} "#, None),
    ] {
        let input = Bytes::from(body.to_owned());
        let output = rewrite(input, "upstream", protocol, effort, false).unwrap();
        assert_eq!(text(&output), body);
    }
}

#[test]
fn model_replacement_uses_json_string_escaping_for_all_protocols() {
    let model = "provider/\"model\\name\n\t雪";
    for protocol in ["chat-completions", "responses", "messages"] {
        let output = rewrite(
            Bytes::from_static(br#"{"model":"alias","input":"hello"}"#),
            model,
            protocol,
            None,
            false,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value, json!({"model":model,"input":"hello"}));
    }
}

#[test]
fn duplicate_and_escaped_model_keys_use_the_last_value_and_emit_one_key() {
    let input = Bytes::from_static(br#"{"model":"first","mo\u0064el":"last","input":false}"#);
    let output = rewrite(input, "last", "messages", None, true).unwrap();
    assert_eq!(text(&output).matches("\"model\"").count(), 1);
    assert!(!text(&output).contains("first"));
    assert_eq!(
        serde_json::from_slice::<Value>(&output).unwrap(),
        json!({"model":"last","input":false})
    );
}

#[test]
fn chat_effort_replaces_duplicate_decoded_keys_without_touching_reasoning() {
    let input = Bytes::from_static(
        br#"{"model":"m","reasoning_effort":"low","reasoning_\u0065ffort":"medium","reasoning":{"effort":"native","extension":0}}"#,
    );
    let output = rewrite(input, "m", "chat-completions", Some("high"), true).unwrap();
    assert_eq!(text(&output).matches("\"reasoning_effort\"").count(), 1);
    assert_eq!(
        serde_json::from_slice::<Value>(&output).unwrap(),
        json!({"model":"m","reasoning_effort":"high","reasoning":{"effort":"native","extension":0}})
    );
}

#[test]
fn responses_replaces_last_reasoning_and_effort_and_preserves_other_members() {
    let input = Bytes::from_static(
        br#"{"model":"m","reasoning":{"discarded":true,"effort":"low"},"reas\u006fning":{"extension":false,"effort":"low","ef\u0066ort":"medium","summary":"auto"}}"#,
    );
    let output = rewrite(input, "m", "responses", Some("high"), true).unwrap();
    assert_eq!(text(&output).matches("\"reasoning\"").count(), 1);
    assert_eq!(text(&output).matches("\"effort\"").count(), 1);
    assert_eq!(
        serde_json::from_slice::<Value>(&output).unwrap(),
        json!({"model":"m","reasoning":{"extension":false,"effort":"high","summary":"auto"}})
    );
}

#[test]
fn supplied_responses_effort_creates_absent_or_null_reasoning() {
    for body in [r#"{"model":"m"}"#, r#"{"model":"m","reasoning":null}"#] {
        let output = rewrite(
            Bytes::from(body.to_owned()),
            "m",
            "responses",
            Some("high"),
            false,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&output).unwrap(),
            json!({"model":"m","reasoning":{"effort":"high"}})
        );
    }
}

#[test]
fn no_effort_does_not_create_defaults_or_translate_native_fields() {
    for protocol in ["chat-completions", "responses", "messages"] {
        let output = rewrite(
            Bytes::from_static(br#"{"model":"alias","reasoning":null,"thinking":{"type":"enabled","budget_tokens":2048}}"#),
            "upstream",
            protocol,
            None,
            false,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&output).unwrap(),
            json!({"model":"upstream","reasoning":null,"thinking":{"type":"enabled","budget_tokens":2048}})
        );
    }
}

#[test]
fn messages_ignores_supplied_effort_without_translating_it() {
    let output = rewrite(
        Bytes::from_static(br#"{"model":"alias","reasoning_effort":"native","reasoning":{"effort":"native"},"thinking":{"budget_tokens":1024}}"#),
        "upstream",
        "messages",
        Some("high"),
        false,
    )
    .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&output).unwrap(),
        json!({"model":"upstream","reasoning_effort":"native","reasoning":{"effort":"native"},"thinking":{"budget_tokens":1024}})
    );
}

#[test]
fn reasoning_extensions_and_input_preserve_raw_spelling() {
    let extension = r#"{ "payload" : "a\u0062\/", "disabled" : false, "zero" : 0 }"#;
    let input_text = r#"{ "type" : "input_image", "image_url" : "data:image/png;base64,YWJj" }"#;
    let body = format!(
        r#"{{"model":"alias","reasoning":{{"effort":"low","extension":{extension}}},"input":[{input_text}]}}"#
    );
    let output = rewrite(
        Bytes::from(body),
        "upstream",
        "responses",
        Some("high"),
        false,
    )
    .unwrap();
    // Unknown subtrees retain internal spaces and the caller's escape spelling.
    assert!(text(&output).contains(extension));
    assert!(text(&output).contains(input_text));
    let value: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["reasoning"]["effort"], "high");
    assert_eq!(value["reasoning"]["extension"]["disabled"], false);
}

#[test]
fn opaque_scalars_and_numeric_spelling_survive_a_model_change() {
    let body = br#"{"model":"alias","disabled":false,"zero":0,"big":18446744073709551615,"fraction":1.2300e+04,"native":{"a":null,"b":[true,false,0]}}"#;
    let output = rewrite(
        Bytes::from_static(body),
        "upstream",
        "messages",
        None,
        false,
    )
    .unwrap();
    assert!(text(&output).contains("18446744073709551615"));
    assert!(text(&output).contains("1.2300e+04"));
    assert!(text(&output).contains(r#""native":{"a":null,"b":[true,false,0]}"#));
    let value: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["disabled"], false);
    assert_eq!(value["zero"], 0);
}

#[test]
fn duplicate_normalization_reaches_unknown_objects_and_array_elements() {
    let body = br#"{"model":"m","native":{"flag":true,"fl\u0061g":false,"array":[{"x":1,"x":0},[{"z":"old","z":"last"}]]},"reasoning":{"effort":"low","effort":"high"}}"#;
    let output = rewrite(
        Bytes::from_static(body),
        "m",
        "responses",
        Some("high"),
        true,
    )
    .unwrap();
    assert_eq!(text(&output).matches("\"flag\"").count(), 1);
    assert_eq!(text(&output).matches("\"x\"").count(), 1);
    assert_eq!(text(&output).matches("\"z\"").count(), 1);
    assert_eq!(text(&output).matches("\"effort\"").count(), 1);
    assert!(!text(&output).contains("old"));
    assert_eq!(
        serde_json::from_slice::<Value>(&output).unwrap(),
        serde_json::from_slice::<Value>(body).unwrap()
    );
}

#[test]
fn duplicate_normalization_honors_last_null_reasoning_before_an_effort_overlay() {
    let input = Bytes::from_static(
        br#"{"model":"m","reasoning":{"extension":"discarded"},"reasoning":null}"#,
    );
    let output = rewrite(input, "m", "responses", Some("high"), true).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&output).unwrap(),
        json!({"model":"m","reasoning":{"effort":"high"}})
    );
}

#[test]
fn normalization_preserves_private_number_map_dom_semantics() {
    let input = br#"{"model":"m","native":{"$serde_json::private::Number":"1e400"},"array":[{"$serde_json::private::Numbe\u0072":"1"}],"ordinary":{"first":false,"$serde_json::private::Number":"2"}}"#;
    let output = rewrite(Bytes::from_static(input), "m", "messages", None, true).unwrap();
    let expected = serde_json::from_slice::<Value>(input).unwrap();
    let actual: BTreeMap<String, &RawValue> = serde_json::from_slice(&output).unwrap();
    for (key, value) in expected.as_object().unwrap() {
        assert_eq!(actual[key].get(), serde_json::to_string(value).unwrap());
    }
}

#[test]
fn private_raw_value_token_remains_an_ordinary_provider_extension() {
    let input = br#"{"model":"alias","native":{"$serde_json::private::RawValue":"[1,false]"}}"#;
    for normalize in [false, true] {
        let output = rewrite(
            Bytes::from_static(input),
            "upstream",
            "messages",
            None,
            normalize,
        )
        .unwrap();
        let body: BTreeMap<String, &RawValue> = serde_json::from_slice(&output).unwrap();
        let extension: BTreeMap<String, &RawValue> =
            serde_json::from_str(body["native"].get()).unwrap();
        assert_eq!(extension.len(), 1);
        assert_eq!(
            extension["$serde_json::private::RawValue"].get(),
            "\"[1,false]\""
        );
    }
}
