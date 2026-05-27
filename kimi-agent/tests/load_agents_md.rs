use tempfile::TempDir;

use kaos::KaosPath;
use kimi_agent::soul::agent::load_agents_md;

#[tokio::test]
async fn test_load_agents_md_found() {
    let dir = TempDir::new().expect("temp dir");
    let work_dir = KaosPath::unsafe_from_local_path(dir.path());
    let agents_md = work_dir.clone() / "AGENTS.md";
    agents_md
        .write_text("Test agents content")
        .await
        .expect("write agents md");

    let content = load_agents_md(&work_dir).await;

    let text = content.expect("agents markdown content");
    assert!(text.contains("<!-- From: "));
    assert!(text.contains("Test agents content"));
}

#[tokio::test]
async fn test_load_agents_md_not_found() {
    let dir = TempDir::new().expect("temp dir");
    let work_dir = KaosPath::unsafe_from_local_path(dir.path());

    let content = load_agents_md(&work_dir).await;

    assert!(content.is_none());
}

#[tokio::test]
async fn test_load_agents_md_lowercase() {
    let dir = TempDir::new().expect("temp dir");
    let work_dir = KaosPath::unsafe_from_local_path(dir.path());
    let agents_md = work_dir.clone() / "agents.md";
    agents_md
        .write_text("Lowercase agents content")
        .await
        .expect("write agents md");

    let content = load_agents_md(&work_dir).await;

    let text = content.expect("agents markdown content");
    assert!(text.contains("<!-- From: "));
    assert!(text.contains("Lowercase agents content"));
}

#[tokio::test]
async fn test_load_agents_md_merges_root_to_leaf_with_kimi_priority() {
    let dir = TempDir::new().expect("temp dir");
    let root = KaosPath::unsafe_from_local_path(dir.path());
    let work_dir = root.clone() / "project" / "nested";
    std::fs::create_dir_all(work_dir.as_path()).expect("create nested directory");
    std::fs::create_dir_all((root.clone() / "project" / ".kimi").as_path())
        .expect("create root .kimi dir");
    std::fs::create_dir_all((work_dir.clone() / ".kimi").as_path())
        .expect("create leaf .kimi dir");

    // Mark project root for discovery.
    (root.clone() / "project" / ".git")
        .write_text("")
        .await
        .expect("write git marker");

    (root.clone() / "project" / ".kimi" / "AGENTS.md")
        .write_text("root kimi")
        .await
        .expect("write root .kimi agents");
    (root.clone() / "project" / "AGENTS.md")
        .write_text("root upper")
        .await
        .expect("write root agents");
    (work_dir.clone() / ".kimi" / "AGENTS.md")
        .write_text("leaf kimi")
        .await
        .expect("write leaf .kimi agents");
    (work_dir.clone() / "AGENTS.md")
        .write_text("leaf upper")
        .await
        .expect("write leaf agents");

    let content = load_agents_md(&work_dir).await.expect("merged agents content");

    assert!(content.contains("root kimi"));
    assert!(content.contains("root upper"));
    assert!(content.contains("leaf kimi"));
    assert!(content.contains("leaf upper"));
}
