use super::*;

fn database() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    crate::store::migrations::apply(&mut connection, crate::store::migrations::MIGRATIONS).unwrap();
    connection
}

fn entry() -> Entry {
    Entry {
        key: Key {
            host: "github.com".into(),
            owner: "admin".into(),
            repository: "repo".into(),
            base: "a".repeat(40),
            head: "b".repeat(40),
        },
        body: "immutable fixture".into(),
        fetched_at: 1,
    }
}

fn access(source_id: &str, scope: &str) -> Access {
    Access {
        source_id: source_id.into(),
        scope: scope.into(),
    }
}

#[test]
fn exact_repository_and_sha_keys_preserve_immutable_results_and_source_access() {
    let mut connection = database();
    let mut entry = entry();
    let original = access("first", "original");
    put(&mut connection, &entry, &original).unwrap();
    entry.body = "replacement refused".into();
    put(&mut connection, &entry, &original).unwrap();
    assert_eq!(
        get(&connection, &entry.key, &original).unwrap().as_deref(),
        Some("immutable fixture")
    );
    let changed = access("first", "changed");
    assert!(get(&connection, &entry.key, &changed).unwrap().is_none());
    put(&mut connection, &entry, &changed).unwrap();
    assert!(get(&connection, &entry.key, &original).unwrap().is_none());
    assert_eq!(
        get(&connection, &entry.key, &changed).unwrap().as_deref(),
        Some("immutable fixture")
    );
    let second = access("second", "scope");
    put(&mut connection, &entry, &second).unwrap();
    for index in 0..5 {
        let mut key = entry.key.clone();
        match index {
            0 => key.host = "gitext.company.com".into(),
            1 => key.owner = "other".into(),
            2 => key.repository = "other".into(),
            3 => key.base = "c".repeat(40),
            _ => key.head = "c".repeat(40),
        }
        assert!(get(&connection, &key, &changed).unwrap().is_none());
    }
    remove_source(&connection, "first").unwrap();
    assert!(get(&connection, &entry.key, &changed).unwrap().is_none());
    assert!(get(&connection, &entry.key, &second).unwrap().is_some());
    remove_source(&connection, "second").unwrap();
    assert_eq!(
        connection
            .query_row::<i64, _, _>("SELECT count(*) FROM github_comparisons", [], |row| row
                .get(0))
            .unwrap(),
        0
    );
}

#[test]
fn store_pruning_removes_comparisons_and_their_access_rows() {
    let mut connection = database();
    let entry = entry();
    put(&mut connection, &entry, &access("source", "scope")).unwrap();
    crate::store::size::enforce(&mut connection, 0).unwrap();
    for table in ["github_comparisons", "github_comparison_scopes"] {
        assert_eq!(
            connection
                .query_row::<i64, _, _>(&format!("SELECT count(*) FROM {table}"), [], |row| row
                    .get(0))
                .unwrap(),
            0
        );
    }
}

type Hooks = std::sync::Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<()>>>;

fn hooks() -> &'static Hooks {
    static HOOKS: std::sync::OnceLock<Hooks> = std::sync::OnceLock::new();
    HOOKS.get_or_init(Default::default)
}

pub(crate) fn subscribe(source: &str) -> tokio::sync::oneshot::Receiver<()> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    hooks().lock().unwrap().insert(source.into(), sender);
    receiver
}

pub(crate) fn writing(source: &str) {
    if let Some(sender) = hooks().lock().unwrap().remove(source) {
        let _ = sender.send(());
    }
}
