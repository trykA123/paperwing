use super::*;

#[test]
fn newly_opened_move_parent_must_match_the_captured_parent_identity() {
    let fixture = Fixture::new("folder-parent-identity-");
    let parent = fixture.0.join("parent");
    std::fs::create_dir(&parent).unwrap();
    std::fs::create_dir(parent.join("source")).unwrap();
    let source = Directory::open(&parent.join("source")).unwrap();
    assert!(source.parent().is_ok());
    std::fs::rename(&parent, fixture.0.join("retained-parent")).unwrap();
    std::fs::create_dir(&parent).unwrap();
    assert!(
        matches!(source.parent(), Err(error) if error.message == "Folder parent identity changed")
    );
    assert!(fixture.0.join("retained-parent/source").is_dir());
    assert!(!parent.join("source").exists());
}

#[test]
fn marked_folder_fixture_is_removed_when_dropped() {
    let fixture = Fixture::new("folder-fixture-drop-");
    let path = fixture.0.clone();
    assert!(path.join(".paperwing-folder-fixture").is_file());
    fixture.repository("repo");
    drop(fixture);
    assert!(!path.exists());
}
