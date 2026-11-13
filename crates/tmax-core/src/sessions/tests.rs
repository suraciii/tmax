use super::*;
use crate::{Request, storage::Store};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

fn request(
    command: &[&str],
    operands: &[&str],
    arguments: &[(&str, &str)],
    dry_run: bool,
) -> Request {
    let mut values = BTreeMap::new();
    for (key, value) in arguments {
        values.insert((*key).to_owned(), vec![(*value).to_owned()]);
    }
    Request {
        command: command.iter().map(|value| (*value).to_owned()).collect(),
        operands: operands.iter().map(|value| (*value).to_owned()).collect(),
        arguments: values,
        dry_run,
        if_revision: None,
    }
}

fn setup(agents: &[(&str, &str, &Path)]) -> (TempDir, Store) {
    let temp = TempDir::new().unwrap();
    let store = Store::at(temp.path().to_owned());
    fs::create_dir_all(&store.root).unwrap();
    let mut config = String::from("version = 1\n");
    for (name, agent, scope) in agents {
        config.push_str(&format!(
            "[agents.{name}]\nagent = \"{agent}\"\nscope = \"{}\"\n",
            scope.display()
        ));
    }
    fs::write(store.root.join("tmax.toml"), config).unwrap();
    (temp, store)
}

fn write_pi_session(root: &Path, id: &str, cwd: Option<&str>, title_text: &str) -> PathBuf {
    let dir = root.join("sessions/--project--");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("2026-01-01T00-00-00Z_{id}.jsonl"));
    let cwd = cwd.map(|value| json!(value)).unwrap_or(Value::Null);
    let lines = [
        json!({"type":"session","version":3,"id":id,"timestamp":"2026-01-01T00:00:00.000Z","cwd":cwd}),
        json!({"type":"message","id":"a","parentId":null,"timestamp":"2026-01-01T00:00:01.000Z","message":{"role":"user","content":[{"type":"text","text":title_text}],"timestamp":1767225601000_i64}}),
        json!({"type":"message","id":"b","parentId":"a","timestamp":"2026-01-01T00:00:02.000Z","message":{"role":"assistant","content":[{"type":"text","text":"answer"}],"timestamp":1767225602000_i64}}),
    ];
    fs::write(
        &path,
        lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    path
}

#[test]
fn list_is_read_only_and_reports_rows_and_shared_targets() {
    let temp = TempDir::new().unwrap();
    let pi_scope = temp.path().join("pi");
    let omp_scope = temp.path().join("omp");
    let (_home, store) = setup(&[
        ("pi-main", "pi", &pi_scope),
        ("omp-main", "omp", &omp_scope),
        ("pi-copy", "pi", &pi_scope),
    ]);
    let source = write_pi_session(&pi_scope, "pi-session", Some("/tmp/project"), "hello");
    let nested = pi_scope.join("sessions/--project--/subagent-artifacts");
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        nested.join("11111111-1111-4111-8111-111111111111_worker_0_transcript.jsonl"),
        json!({"type":"session","version":3,"id":"nested","timestamp":"2026-01-01T00:00:00.000Z"})
            .to_string(),
    )
    .unwrap();
    let before = fs::read(&source).unwrap();
    let value = list(&store, &request(&["session", "list"], &[], &[], false)).unwrap();
    let rows = value["sessions"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["native_id"], "pi-session");
    assert!(!rows.iter().any(|row| row["native_id"] == "nested"));
    assert_eq!(rows[0]["target"], "pi-copy");
    assert_eq!(rows[0]["targets"], json!(["pi-copy", "pi-main"]));
    assert_eq!(
        rows[0]["store"],
        pi_scope.join("sessions").display().to_string()
    );
    assert_eq!(fs::read(source).unwrap(), before);
    assert_eq!(value["coverage"], "partial");
    assert_eq!(value["counts"]["sessions"], 1);
    assert_eq!(value["counts"]["returned"], 1);
    assert!(
        value["warnings"].as_array().unwrap().iter().any(|warning| {
            warning["code"] == "target_absent" && warning["target"] == "omp-main"
        })
    );
    let explicit = list(
        &store,
        &request(
            &["session", "list"],
            &[],
            &[("agent", "pi"), ("path", pi_scope.to_str().unwrap())],
            false,
        ),
    )
    .unwrap();
    assert_eq!(explicit["sessions"][0]["target"], Value::Null);
    assert_eq!(explicit["sessions"][0]["targets"], json!([]));
    assert_eq!(explicit["sessions"][0]["scope"], pi_scope.to_str().unwrap());
    assert_eq!(explicit["targets"][0]["target"], Value::Null);
    let filtered = list(
        &store,
        &request(&["session", "list"], &[], &[("target", "pi-main")], false),
    )
    .unwrap();
    assert_eq!(filtered["sessions"][0]["target"], "pi-copy");
    assert_eq!(
        filtered["sessions"][0]["targets"],
        json!(["pi-copy", "pi-main"])
    );
    write_pi_session(&pi_scope, "pi-no-cwd", None, "no cwd");
    let unknown = list(
        &store,
        &request(
            &["session", "list"],
            &[],
            &[("cwd", "/no/such/project")],
            false,
        ),
    )
    .unwrap();
    assert_eq!(unknown["sessions"].as_array().unwrap().len(), 0);
    assert_eq!(unknown["omitted"]["sessions_excluded_by_unknown_cwd"], 1);
    assert_eq!(
        unknown["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|warning| warning["code"] == "cwd_unknown")
            .unwrap()["count"],
        1
    );
}

#[test]
fn list_cwd_filter_and_unknown_cwd_are_explicit() {
    let temp = TempDir::new().unwrap();
    let scope = temp.path().join("pi");
    let (_home, store) = setup(&[("pi-main", "pi", &scope)]);
    write_pi_session(&scope, "with-cwd", Some("/tmp/project"), "with cwd");
    write_pi_session(&scope, "without-cwd", None, "without cwd");
    let value = list(
        &store,
        &request(&["session", "list"], &[], &[("cwd", "/tmp/project")], false),
    )
    .unwrap();
    assert_eq!(value["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(value["sessions"][0]["native_id"], "with-cwd");
    assert_eq!(value["targets"][0]["cwd_unknown"], 1);
    assert_eq!(value["omitted"]["sessions_excluded_by_unknown_cwd"], 1);
    assert!(
        value["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["code"] == "cwd_unknown")
    );
}

#[test]
fn import_creates_one_native_session_and_detects_repeat() {
    let temp = TempDir::new().unwrap();
    let pi_scope = temp.path().join("pi");
    let omp_scope = temp.path().join("omp");
    let (_home, store) = setup(&[
        ("pi-main", "pi", &pi_scope),
        ("omp-main", "omp", &omp_scope),
    ]);
    write_pi_session(
        &pi_scope,
        "source",
        Some(temp.path().to_str().unwrap()),
        "source prompt",
    );
    let dry = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", "source"],
            &[("to", "omp-main")],
            true,
        ),
    )
    .unwrap();
    assert_eq!(dry["visibility"], "not_attempted");
    assert_eq!(dry["destination"]["native_id"], Value::Null);
    assert!(!omp_scope.join("sessions").exists());

    let value = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", "source"],
            &[("to", "omp-main")],
            false,
        ),
    )
    .unwrap();
    assert_eq!(value["visibility"], "verified");
    let id = value["destination"]["native_id"].as_str().unwrap();
    let listed = list(&store, &request(&["session", "list"], &[], &[], false)).unwrap();
    let rows = listed["sessions"].as_array().unwrap();
    assert!(rows.iter().any(|row| {
        row["target"] == "omp-main"
            && row["native_id"] == id
            && row["imported_from"]["target"] == "pi-main"
    }));
    let repeat = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", "source"],
            &[("to", "omp-main")],
            false,
        ),
    );
    assert_eq!(repeat.unwrap_err().code, "already_imported");
    let again = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", "source"],
            &[("to", "omp-main"), ("again", "true")],
            false,
        ),
    )
    .unwrap();
    assert_eq!(again["visibility"], "verified");
    assert_eq!(again["destination"]["title"], "source prompt (2)");
    assert_ne!(again["destination"]["native_id"], id);
}

#[test]
fn import_without_cwd_fails_and_explicit_cwd_succeeds() {
    let temp = TempDir::new().unwrap();
    let pi_scope = temp.path().join("pi");
    let omp_scope = temp.path().join("omp");
    let (_home, store) = setup(&[
        ("pi-main", "pi", &pi_scope),
        ("omp-main", "omp", &omp_scope),
    ]);
    write_pi_session(&pi_scope, "source", None, "source prompt");
    let error = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", "source"],
            &[("to", "omp-main")],
            false,
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "source_cwd_missing");
    let work = temp.path().join("work");
    fs::create_dir(&work).unwrap();
    let value = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", "source"],
            &[("to", "omp-main"), ("cwd", work.to_str().unwrap())],
            false,
        ),
    )
    .unwrap();
    assert_eq!(value["destination"]["cwd"], work.to_str().unwrap());
}

#[test]
fn unsupported_pi_schema_is_listed_not_silently_dropped() {
    let temp = TempDir::new().unwrap();
    let scope = temp.path().join("pi");
    let (_home, store) = setup(&[("pi-main", "pi", &scope)]);
    let dir = scope.join("sessions/--project--");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("2026-01-01T00-00-00Z_future.jsonl"),
        json!({"type":"session","version":9,"id":"future","cwd":null}).to_string(),
    )
    .unwrap();
    let value = list(&store, &request(&["session", "list"], &[], &[], false)).unwrap();
    assert_eq!(value["sessions"][0]["state"], "unsupported_version");
    assert_eq!(value["targets"][0]["state"], "unsupported");
}

fn sqlite(db: &Path, sql: &str) {
    let status = std::process::Command::new("sqlite3")
        .arg(db)
        .arg(sql)
        .status()
        .unwrap();
    assert!(status.success());
}

fn write_codex_session(home: &Path, work: &Path, id: &str) -> PathBuf {
    let day = home.join("sessions/2026/01/01");
    fs::create_dir_all(&day).unwrap();
    let rollout = day.join(format!("rollout-2026-01-01T00-00-00-{id}.jsonl"));
    let lines = [
        json!({"type":"session_meta","timestamp":"2026-01-01T00:00:00.000Z","payload":{"id":id,"session_id":id,"cwd":work,"timestamp":"2026-01-01T00:00:00.000Z","runtime_workspace_roots":[work],"model_provider":"openai"}}),
        json!({"type":"turn_context","timestamp":"2026-01-01T00:00:00.500Z","payload":{"model":"gpt-turn"}}),
        json!({"type":"response_item","timestamp":"2026-01-01T00:00:01.000Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"codex source"}]}}),
        json!({"type":"response_item","timestamp":"2026-01-01T00:00:02.000Z","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"codex answer"}]}}),
    ];
    fs::write(
        &rollout,
        lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let db = home.join("state_5.sqlite");
    sqlite(
        &db,
        &format!(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, cwd TEXT NOT NULL, title TEXT NOT NULL, archived INTEGER NOT NULL, model_provider TEXT, model TEXT, thread_source TEXT); INSERT INTO threads VALUES ('{id}', '{}', 1767225600, 1767225602, '{}', 'Codex title', 0, 'openai', 'gpt-test', 'user'); INSERT INTO threads VALUES ('codex-subagent', '{}', 1767225601, 1767225603, '{}', 'Subagent title', 0, 'openai', 'gpt-test', 'subagent');",
            rollout.display(),
            work.display(),
            rollout.display(),
            work.display()
        ),
    );
    rollout
}

#[test]
fn codex_index_lists_and_imports_without_rewriting_source() {
    let temp = TempDir::new().unwrap();
    let codex_scope = temp.path().join("codex");
    let omp_scope = temp.path().join("omp");
    let work = temp.path().join("work");
    fs::create_dir(&work).unwrap();
    let (_home, store) = setup(&[
        ("codex-main", "codex", &codex_scope),
        ("omp-main", "omp", &omp_scope),
    ]);
    let rollout = write_codex_session(&codex_scope, &work, "codex-source");
    let before = fs::read(&rollout).unwrap();
    let value = list(&store, &request(&["session", "list"], &[], &[], false)).unwrap();
    let row = value["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["native_id"] == "codex-source")
        .unwrap();
    assert_eq!(row["agent"], "codex");
    assert_eq!(row["title"], "Codex title");
    assert_eq!(row["model"], "openai/gpt-test");
    assert!(
        !value["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["native_id"] == "codex-subagent")
    );
    let imported = import(
        &store,
        &request(
            &["session", "import"],
            &["codex-main", "codex-source"],
            &[("to", "omp-main")],
            false,
        ),
    )
    .unwrap();
    assert_eq!(imported["visibility"], "verified");
    assert_eq!(imported["source"]["model"], "openai/gpt-turn");
    assert_eq!(fs::read(rollout).unwrap(), before);
}

#[test]
fn hermes_and_opencode_current_schemas_are_read_without_legacy_union() {
    let temp = TempDir::new().unwrap();
    let hermes = temp.path().join("hermes");
    let opencode = temp.path().join("opencode");
    fs::create_dir_all(&hermes).unwrap();
    fs::create_dir_all(&opencode).unwrap();
    let (_home, store) = setup(&[
        ("hermes-main", "hermes", &hermes),
        ("opencode-main", "opencode", &opencode),
    ]);
    let hermes_db = hermes.join("state.db");
    sqlite(
        &hermes_db,
        "CREATE TABLE sessions (id TEXT PRIMARY KEY, model TEXT, started_at REAL NOT NULL, message_count INTEGER, cwd TEXT, title TEXT, archived INTEGER NOT NULL, hidden INTEGER NOT NULL, last_activity_at REAL, tool_call_count INTEGER); INSERT INTO sessions VALUES ('hermes-1','model-x',1767225600,2,'/tmp', 'Hermes title',0,1,1767225601,1);",
    );
    let opencode_data = store.home.join(".local/share/opencode");
    fs::create_dir_all(&opencode_data).unwrap();
    let opencode_db = opencode_data.join("opencode.db");
    sqlite(
        &opencode_db,
        "CREATE TABLE session_v2 (id TEXT PRIMARY KEY, directory TEXT NOT NULL, title TEXT, model TEXT, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, parent_id TEXT, time_archived INTEGER, time_suspended INTEGER); CREATE TABLE session (id TEXT PRIMARY KEY); INSERT INTO session_v2 VALUES ('oc-1','/tmp','OpenCode title','{\"id\":\"m\",\"providerID\":\"p\"}',1767225600000,1767225601000,NULL,0,NULL); INSERT INTO session VALUES ('legacy-1');",
    );
    let value = list(&store, &request(&["session", "list"], &[], &[], false)).unwrap();
    let rows = value["sessions"].as_array().unwrap();
    assert!(rows.iter().any(|row| {
        row["native_id"] == "hermes-1" && row["hidden"] == true && row["message_count"] == 2
    }));
    assert!(rows.iter().any(|row| {
        row["native_id"] == "oc-1"
            && row["model"] == "p/m"
            && row["state"] == "readable"
            && row["archived"] == false
            && row["hidden"].is_null()
    }));
    assert!(!rows.iter().any(|row| row["native_id"] == "legacy-1"));
}

#[test]
fn codex_destination_writes_a_rollout_and_native_index_row() {
    let temp = TempDir::new().unwrap();
    let pi_scope = temp.path().join("pi");
    let codex_scope = temp.path().join("codex");
    let work = temp.path().join("work");
    fs::create_dir_all(&codex_scope).unwrap();
    fs::create_dir(&work).unwrap();
    let (_home, store) = setup(&[
        ("pi-main", "pi", &pi_scope),
        ("codex-main", "codex", &codex_scope),
    ]);
    write_pi_session(
        &pi_scope,
        "source",
        Some(work.to_str().unwrap()),
        "codex target",
    );
    sqlite(
        &codex_scope.join("state_5.sqlite"),
        "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, source TEXT NOT NULL, model_provider TEXT NOT NULL, cwd TEXT NOT NULL, title TEXT NOT NULL, sandbox_policy TEXT NOT NULL, approval_mode TEXT NOT NULL);",
    );
    let value = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", "source"],
            &[("to", "codex-main")],
            false,
        ),
    )
    .unwrap();
    let id = value["destination"]["native_id"].as_str().unwrap();
    let db = codex_scope.join("state_5.sqlite");
    let rows = sqlite_json(
        &db,
        &format!("SELECT rollout_path FROM threads WHERE id='{id}';"),
    )
    .unwrap();
    let rollout = rows[0]["rollout_path"].as_str().unwrap();
    assert!(Path::new(rollout).is_file());
    let listed = list(
        &store,
        &request(
            &["session", "list"],
            &[],
            &[("target", "codex-main")],
            false,
        ),
    )
    .unwrap();
    assert!(
        listed["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["native_id"] == id && row["state"] == "readable")
    );
}

#[test]
fn hermes_and_opencode_sources_import_visible_text_into_omp() {
    let temp = TempDir::new().unwrap();
    let hermes = temp.path().join("hermes");
    let opencode = temp.path().join("opencode");
    let omp_scope = temp.path().join("omp");
    let work = temp.path().join("work");
    fs::create_dir_all(&hermes).unwrap();
    fs::create_dir_all(&opencode).unwrap();
    fs::create_dir(&work).unwrap();
    let (_home, store) = setup(&[
        ("hermes-main", "hermes", &hermes),
        ("opencode-main", "opencode", &opencode),
        ("omp-main", "omp", &omp_scope),
    ]);
    let hermes_db = hermes.join("state.db");
    sqlite(
        &hermes_db,
        &format!(
            "CREATE TABLE sessions (id TEXT PRIMARY KEY, model TEXT, started_at REAL NOT NULL, message_count INTEGER, cwd TEXT, title TEXT, archived INTEGER NOT NULL, hidden INTEGER NOT NULL, last_activity_at REAL, tool_call_count INTEGER); CREATE TABLE messages (id INTEGER PRIMARY KEY, session_id TEXT, role TEXT, content TEXT, tool_name TEXT, tool_calls TEXT, timestamp REAL, active INTEGER, reasoning TEXT); INSERT INTO sessions VALUES ('hermes-1','x',1767225600,2,'{}','Hermes title',0,0,1767225601,0); INSERT INTO messages VALUES (1,'hermes-1','user','hello',NULL,NULL,1767225600,1,NULL); INSERT INTO messages VALUES (2,'hermes-1','assistant','answer',NULL,NULL,1767225601,1,'hidden reasoning');",
            work.display()
        ),
    );
    let opencode_db = store.home.join(".local/share/opencode/opencode.db");
    fs::create_dir_all(opencode_db.parent().unwrap()).unwrap();
    sqlite(
        &opencode_db,
        &format!(
            "CREATE TABLE session_v2 (id TEXT PRIMARY KEY, directory TEXT NOT NULL, title TEXT, model TEXT, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL); CREATE TABLE session_message (id TEXT PRIMARY KEY, session_id TEXT, type TEXT, seq INTEGER, time_created INTEGER, data TEXT); INSERT INTO session_v2 VALUES ('oc-1','{}','OpenCode title','{{\"id\":\"m\",\"providerID\":\"p\"}}',1767225600000,1767225601000); INSERT INTO session_message VALUES ('a','oc-1','user',0,1767225600000,'{{\"text\":\"hello\"}}'); INSERT INTO session_message VALUES ('b','oc-1','assistant',1,1767225601000,'{{\"content\":[{{\"type\":\"text\",\"text\":\"answer\"}}]}}');",
            work.display()
        ),
    );
    for (source, id) in [("hermes-main", "hermes-1"), ("opencode-main", "oc-1")] {
        let value = import(
            &store,
            &request(
                &["session", "import"],
                &[source, id],
                &[("to", "omp-main")],
                false,
            ),
        )
        .unwrap();
        assert_eq!(value["visibility"], "verified");
        assert_eq!(value["source"]["target"], source);
        if source == "hermes-main" {
            assert_eq!(value["omitted"]["reasoning"], 1);
        }
    }
}

#[test]
fn dsh_v4_source_lists_and_imports_without_changing_compressed_source() {
    let temp = TempDir::new().unwrap();
    let dsh = temp.path().join("dsh");
    let omp_scope = temp.path().join("omp");
    let work = temp.path().join("work");
    fs::create_dir(&work).unwrap();
    let (_home, store) = setup(&[("dsh-main", "dsh", &dsh), ("omp-main", "omp", &omp_scope)]);
    let dir = dsh.join("sessions/--project--/session-dsh-1");
    fs::create_dir_all(&dir).unwrap();
    let raw = dir.join("session.jsonl");
    let lines = [
        json!({"type":"session","id":"session-dsh-1","cwd":work,"createdAt":"2026-01-01T00:00:00.000Z"}),
        json!({"type":"session/title","seq":1,"time":1767225600000_i64,"data":{"title":"DSH title"}}),
        json!({"type":"user/message","seq":2,"time":1767225600000_i64,"data":{"role":"user","source":{"kind":"user"},"content":[{"type":"text","text":"hello"}]}}),
        json!({"type":"assistant/message","seq":3,"time":1767225601000_i64,"data":{"message":{"role":"assistant","content":[{"type":"text","text":"answer"}]}}}),
    ];
    fs::write(
        &raw,
        lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let compressed = dir.join("session.v4.jsonl.zstd");
    let status = std::process::Command::new("zstd")
        .args(["-q", "-o"])
        .arg(&compressed)
        .arg(&raw)
        .status()
        .unwrap();
    assert!(status.success());
    fs::remove_file(raw).unwrap();
    let before = fs::read(&compressed).unwrap();
    let value = list(&store, &request(&["session", "list"], &[], &[], false)).unwrap();
    assert!(
        value["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| { row["native_id"] == "session-dsh-1" && row["title"] == "DSH title" })
    );
    let imported = import(
        &store,
        &request(
            &["session", "import"],
            &["dsh-main", "session-dsh-1"],
            &[("to", "omp-main")],
            false,
        ),
    )
    .unwrap();
    assert_eq!(imported["visibility"], "verified");
    assert_eq!(fs::read(compressed).unwrap(), before);
}

#[test]
fn tail_keeps_the_first_prompt_and_recent_turns() {
    let temp = TempDir::new().unwrap();
    let pi_scope = temp.path().join("pi");
    let omp_scope = temp.path().join("omp");
    let (_home, store) = setup(&[
        ("pi-main", "pi", &pi_scope),
        ("omp-main", "omp", &omp_scope),
    ]);
    let work = store.home.join("work");
    fs::create_dir(&work).unwrap();
    let dir = pi_scope.join("sessions/--project--");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("2026-01-01T00-00-00Z_source.jsonl");
    let mut lines = vec![
        json!({"type":"session","version":3,"id":"source","timestamp":"2026-01-01T00:00:00.000Z","cwd":work}),
    ];
    let mut parent: Option<String> = None;
    for turn in 0..3 {
        for (role, text) in [
            ("user", format!("user {turn}")),
            ("assistant", format!("answer {turn}")),
        ] {
            let id = format!("{turn}-{role}");
            lines.push(json!({
                "type":"message",
                "id":id,
                "parentId":parent,
                "timestamp":format!("2026-01-01T00:00:0{}.000Z", turn * 2 + if role == "user" { 1 } else { 2 }),
                "message":{"role":role,"content":[{"type":"text","text":text}],"timestamp":1767225600000_i64},
            }));
            parent = Some(id);
        }
    }
    fs::write(
        &path,
        lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let value = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", "source"],
            &[("to", "omp-main"), ("tail", "1")],
            false,
        ),
    )
    .unwrap();
    assert_eq!(value["counts"]["turns"], 2);
    assert_eq!(value["omitted"]["turns_omitted_by_tail"], 1);
    let destination = resolve_store(&store, "omp", &omp_scope).unwrap();
    let readback = readers::read_source(
        &destination,
        value["destination"]["native_id"].as_str().unwrap(),
    )
    .unwrap();
    let text = portable_text(&readback).join("|");
    assert!(text.contains("user 0"));
    assert!(text.contains("answer 2"));
    assert!(!text.contains("answer 1"));
    assert!(omp_scope.join("sessions/-work").is_dir());
    let conflict = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", "source"],
            &[
                ("to", "omp-main"),
                ("tail", "1"),
                ("allow-over-budget", "true"),
            ],
            false,
        ),
    );
    assert_eq!(conflict.unwrap_err().code, "invalid_arguments");
    let full = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", "source"],
            &[("to", "omp-main"), ("tail", "2"), ("again", "true")],
            false,
        ),
    )
    .unwrap();
    assert!(full["omitted"].get("turns_omitted_by_tail").is_none());
    let destination = resolve_store(&store, "omp", &omp_scope).unwrap();
    let readback = readers::read_source(
        &destination,
        full["destination"]["native_id"].as_str().unwrap(),
    )
    .unwrap();
    let text = portable_text(&readback).join("|");
    assert!(text.contains("answer 0"));
    assert!(text.contains("answer 1"));
    assert!(text.contains("answer 2"));
    assert!(!text.contains("tmax omission marker"));
}

#[test]
#[cfg(unix)]
fn symlinked_scope_cannot_enter_tmax_storage() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let native = temp.path().join("native");
    fs::create_dir_all(&native).unwrap();
    let (_home, store) = setup(&[("pi-main", "pi", &native)]);
    let inside = store.root.join("native");
    fs::create_dir_all(&inside).unwrap();
    let link = temp.path().join("pi-link");
    symlink(&inside, &link).unwrap();
    let error = resolve_store(&store, "pi", &link).unwrap_err();
    assert_eq!(error.code, "invalid_native_scope");
}

#[test]
fn same_store_and_unqualified_destination_fail_before_effects() {
    let temp = TempDir::new().unwrap();
    let pi_scope = temp.path().join("pi");
    let hermes = temp.path().join("hermes");
    let (_home, store) = setup(&[
        ("pi-main", "pi", &pi_scope),
        ("pi-copy", "pi", &pi_scope),
        ("omp-main", "omp", &pi_scope),
        ("hermes-main", "hermes", &hermes),
        ("dsh-main", "dsh", &hermes),
    ]);
    write_pi_session(
        &pi_scope,
        "source",
        Some(temp.path().to_str().unwrap()),
        "source",
    );
    let same = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", "source"],
            &[("to", "pi-copy")],
            false,
        ),
    )
    .unwrap_err();
    assert_eq!(same.code, "same_session_store");
    let shared = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", "source"],
            &[("to", "omp-main")],
            false,
        ),
    )
    .unwrap_err();
    assert_eq!(shared.code, "same_session_store");
    let listed = list(&store, &request(&["session", "list"], &[], &[], false)).unwrap();
    let rows = listed["sessions"].as_array().unwrap();
    assert_eq!(
        rows.iter()
            .filter(|row| row["native_id"] == "source")
            .count(),
        1
    );
    let row = rows
        .iter()
        .find(|row| row["native_id"] == "source")
        .unwrap();
    assert_eq!(row["targets"], json!(["omp-main", "pi-copy", "pi-main"]));
    let unsupported = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", "source"],
            &[("to", "hermes-main")],
            false,
        ),
    )
    .unwrap_err();
    assert_eq!(unsupported.code, "unsupported_destination");
    let dsh = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", "source"],
            &[("to", "dsh-main")],
            false,
        ),
    )
    .unwrap_err();
    assert_eq!(dsh.code, "unsupported_destination");
}

#[test]
fn opencode_destination_requires_a_provider_qualified_source_model() {
    let temp = TempDir::new().unwrap();
    let hermes = temp.path().join("hermes");
    let opencode = temp.path().join("opencode");
    fs::create_dir_all(&hermes).unwrap();
    fs::create_dir_all(&opencode).unwrap();
    let (_home, store) = setup(&[
        ("hermes-main", "hermes", &hermes),
        ("opencode-main", "opencode", &opencode),
    ]);
    sqlite(
        &hermes.join("state.db"),
        "CREATE TABLE sessions (id TEXT PRIMARY KEY, model TEXT, started_at REAL NOT NULL, message_count INTEGER, cwd TEXT, title TEXT, archived INTEGER NOT NULL, hidden INTEGER NOT NULL, last_activity_at REAL, tool_call_count INTEGER); CREATE TABLE messages (id INTEGER PRIMARY KEY, session_id TEXT, role TEXT, content TEXT, tool_name TEXT, tool_calls TEXT, timestamp REAL, active INTEGER); INSERT INTO sessions VALUES ('bare-model','m',1767225600,2,'/tmp','Bare model',0,0,1767225601,0); INSERT INTO messages VALUES (1,'bare-model','user','hello',NULL,NULL,1767225600,1); INSERT INTO messages VALUES (2,'bare-model','assistant','answer',NULL,NULL,1767225601,1);",
    );
    let error = import(
        &store,
        &request(
            &["session", "import"],
            &["hermes-main", "bare-model"],
            &[("to", "opencode-main")],
            true,
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "model_unknown");
}

#[test]
fn opencode_destination_uses_the_official_import_and_reads_back() {
    let Ok(version) = std::process::Command::new("opencode")
        .arg("--version")
        .output()
    else {
        return;
    };
    let version = String::from_utf8_lossy(&version.stdout);
    if !matches!(
        version.split_whitespace().last(),
        Some("2.0.28" | "v2.0.28")
    ) {
        return;
    }
    let temp = TempDir::new().unwrap();
    let hermes = temp.path().join("hermes");
    let opencode = temp.path().join("opencode");
    let opencode_db = opencode.join("opencode.db");
    let work = temp.path().join("work");
    fs::create_dir_all(&hermes).unwrap();
    fs::create_dir_all(&opencode).unwrap();
    fs::create_dir(&work).unwrap();
    let (_home, store) = setup(&[
        ("hermes-main", "hermes", &hermes),
        ("opencode-main", "opencode", &opencode_db),
    ]);
    let hermes_db = hermes.join("state.db");
    sqlite(
        &hermes_db,
        &format!(
            "CREATE TABLE sessions (id TEXT PRIMARY KEY, model TEXT, started_at REAL NOT NULL, message_count INTEGER, cwd TEXT, title TEXT, archived INTEGER NOT NULL, hidden INTEGER NOT NULL, last_activity_at REAL, tool_call_count INTEGER); CREATE TABLE messages (id INTEGER PRIMARY KEY, session_id TEXT, role TEXT, content TEXT, tool_name TEXT, tool_calls TEXT, timestamp REAL, active INTEGER); INSERT INTO sessions VALUES ('hermes-source','p/m',1767225600,2,'{}','OpenCode destination',0,0,1767225601,0); INSERT INTO messages VALUES (1,'hermes-source','user','hello',NULL,NULL,1767225600,1); INSERT INTO messages VALUES (2,'hermes-source','assistant','answer',NULL,NULL,1767225601,1);",
            work.display()
        ),
    );
    let value = import(
        &store,
        &request(
            &["session", "import"],
            &["hermes-main", "hermes-source"],
            &[("to", "opencode-main")],
            false,
        ),
    )
    .unwrap();
    assert_eq!(value["visibility"], "verified");
    let id = value["destination"]["native_id"].as_str().unwrap();
    assert!(id.starts_with("ses_"));
    assert_eq!(value["source"]["target"], "hermes-main");
    assert_eq!(value["destination"]["cwd"], work.to_str().unwrap());
    assert_eq!(value["destination"]["title"], "OpenCode destination");
    let listed = list(
        &store,
        &request(
            &["session", "list"],
            &[],
            &[("target", "opencode-main")],
            false,
        ),
    )
    .unwrap();
    assert!(listed["sessions"].as_array().unwrap().iter().any(|row| {
        row["native_id"] == id
            && row["model"] == "p/m"
            && row["title"] == "OpenCode destination"
            && row["state"] == "readable"
    }));
}

#[test]
fn pi_source_selects_the_latest_visible_branch() {
    let temp = TempDir::new().unwrap();
    let pi_scope = temp.path().join("pi");
    let omp_scope = temp.path().join("omp");
    let work = temp.path().join("work");
    fs::create_dir(&work).unwrap();
    let (_home, store) = setup(&[
        ("pi-main", "pi", &pi_scope),
        ("omp-main", "omp", &omp_scope),
    ]);
    let dir = pi_scope.join("sessions/--project--");
    fs::create_dir_all(&dir).unwrap();
    let id = "branch-source";
    let path = dir.join(format!("2026-01-01T00-00-00Z_{id}.jsonl"));
    let lines = [
        json!({"type":"session","version":3,"id":id,"timestamp":"2026-01-01T00:00:00.000Z","cwd":work}),
        json!({"type":"message","id":"a","parentId":null,"timestamp":"2026-01-01T00:00:01.000Z","message":{"role":"user","content":[{"type":"text","text":"root prompt"}]}}),
        json!({"type":"message","id":"b","parentId":"a","timestamp":"2026-01-01T00:00:02.000Z","message":{"role":"assistant","content":[{"type":"text","text":"root answer"}]}}),
        json!({"type":"message","id":"c","parentId":"b","timestamp":"2026-01-01T00:00:03.000Z","message":{"role":"user","content":[{"type":"text","text":"old branch"}]}}),
        json!({"type":"message","id":"d","parentId":"c","timestamp":"2026-01-01T00:00:04.000Z","message":{"role":"assistant","content":[{"type":"text","text":"old answer"}]}}),
        json!({"type":"message","id":"e","parentId":"b","timestamp":"2026-01-01T00:00:05.000Z","message":{"role":"user","content":[{"type":"text","text":"new branch"}]}}),
        json!({"type":"message","id":"f","parentId":"e","timestamp":"2026-01-01T00:00:06.000Z","message":{"role":"assistant","content":[{"type":"text","text":"new answer"}]}}),
    ];
    fs::write(
        &path,
        lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let value = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", id],
            &[("to", "omp-main")],
            false,
        ),
    )
    .unwrap();
    assert_eq!(value["destination"]["title"], "root prompt");
    assert_eq!(value["counts"]["turns"], 2);
    assert_eq!(value["counts"]["messages"], 4);
    assert_eq!(value["omitted"]["other_branches"], 2);
}

#[test]
fn an_unverified_file_based_destination_is_removed() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("session.jsonl");
    fs::write(&path, "{}").unwrap();
    let written = Written {
        native_id: "unverified".into(),
        title: "title".into(),
        cwd: "/tmp".into(),
        schema: "pi.session.v3".into(),
        resume: None,
        cleanup_path: Some(path.clone()),
    };
    let error = verification_failure(&written, "pi-main");
    assert_eq!(error.code, "native_write_failed");
    assert!(!path.exists());
}

#[test]
fn explicit_scope_listing_does_not_require_a_declared_target_config() {
    let temp = TempDir::new().unwrap();
    let scope = temp.path().join("pi");
    let store = Store::at(temp.path().join("home"));
    write_pi_session(&scope, "explicit", Some("/tmp"), "explicit prompt");
    let value = list(
        &store,
        &request(
            &["session", "list"],
            &[],
            &[("agent", "pi"), ("path", scope.to_str().unwrap())],
            false,
        ),
    )
    .unwrap();
    assert_eq!(value["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(value["sessions"][0]["native_id"], "explicit");
    assert_eq!(value["sessions"][0]["target"], Value::Null);
}

#[test]
fn pi_destination_writes_a_readable_native_session() {
    let temp = TempDir::new().unwrap();
    let hermes = temp.path().join("hermes");
    let pi = temp.path().join("pi");
    let work = temp.path().join("work");
    fs::create_dir_all(&hermes).unwrap();
    fs::create_dir_all(&pi).unwrap();
    fs::create_dir(&work).unwrap();
    let (_home, store) = setup(&[("hermes-main", "hermes", &hermes), ("pi-main", "pi", &pi)]);
    sqlite(
        &hermes.join("state.db"),
        &format!(
            "CREATE TABLE sessions (id TEXT PRIMARY KEY, model TEXT, started_at REAL NOT NULL, message_count INTEGER, cwd TEXT, title TEXT, archived INTEGER NOT NULL, hidden INTEGER NOT NULL, last_activity_at REAL, tool_call_count INTEGER); CREATE TABLE messages (id INTEGER PRIMARY KEY, session_id TEXT, role TEXT, content TEXT, tool_name TEXT, tool_calls TEXT, timestamp REAL, active INTEGER); INSERT INTO sessions VALUES ('hermes-source','m',1767225600,2,'{}','Pi destination',0,0,1767225601,0); INSERT INTO messages VALUES (1,'hermes-source','user','hello',NULL,NULL,1767225600,1); INSERT INTO messages VALUES (2,'hermes-source','assistant','answer',NULL,NULL,1767225601,1);",
            work.display()
        ),
    );
    let value = import(
        &store,
        &request(
            &["session", "import"],
            &["hermes-main", "hermes-source"],
            &[("to", "pi-main")],
            false,
        ),
    )
    .unwrap();
    assert_eq!(value["visibility"], "verified");
    let id = value["destination"]["native_id"].as_str().unwrap();
    assert_eq!(value["destination"]["title"], "Pi destination");
    let listed = list(
        &store,
        &request(&["session", "list"], &[], &[("target", "pi-main")], false),
    )
    .unwrap();
    assert!(listed["sessions"].as_array().unwrap().iter().any(|row| {
        row["native_id"] == id && row["state"] == "readable" && row["title"] == "Pi destination"
    }));
}

#[test]
fn destination_pi_omp_schema_is_qualified_before_writes() {
    let temp = TempDir::new().unwrap();
    let pi_scope = temp.path().join("pi");
    let omp_scope = temp.path().join("omp");
    let (_home, store) = setup(&[
        ("pi-main", "pi", &pi_scope),
        ("omp-main", "omp", &omp_scope),
    ]);
    write_pi_session(
        &pi_scope,
        "source",
        Some(temp.path().to_str().unwrap()),
        "source",
    );
    let dir = omp_scope.join("sessions/--project--");
    fs::create_dir_all(&dir).unwrap();
    let v4 = dir.join("2026-01-01T00-00-00Z_v4.jsonl");
    fs::write(
        &v4,
        json!({"type":"session","version":4,"id":"v4"}).to_string(),
    )
    .unwrap();
    let error = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", "source"],
            &[("to", "omp-main")],
            false,
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "native_schema_unsupported");
    assert_eq!(
        super::model::files_under(&omp_scope.join("sessions"), ".jsonl", 10)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn codex_dry_run_requires_a_qualified_destination_index() {
    let temp = TempDir::new().unwrap();
    let pi_scope = temp.path().join("pi");
    let codex_scope = temp.path().join("codex");
    let (_home, store) = setup(&[
        ("pi-main", "pi", &pi_scope),
        ("codex-main", "codex", &codex_scope),
    ]);
    write_pi_session(
        &pi_scope,
        "source",
        Some(temp.path().to_str().unwrap()),
        "source",
    );
    let error = import(
        &store,
        &request(
            &["session", "import"],
            &["pi-main", "source"],
            &[("to", "codex-main")],
            true,
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "native_schema_unsupported");
}

#[test]
fn compressed_sources_are_rejected_after_bounded_decompression() {
    let temp = TempDir::new().unwrap();
    let raw = temp.path().join("raw.jsonl");
    let compressed = temp.path().join("session.v4.jsonl.zstd");
    fs::File::create(&raw)
        .unwrap()
        .set_len(super::model::MAX_SESSION_BYTES + 1)
        .unwrap();
    let status = std::process::Command::new("zstd")
        .args(["-q", "-o"])
        .arg(&compressed)
        .arg(&raw)
        .status()
        .unwrap();
    assert!(status.success());
    let error = super::model::zstd_decompress(&compressed).unwrap_err();
    assert_eq!(error.code, "source_oversized");
}
