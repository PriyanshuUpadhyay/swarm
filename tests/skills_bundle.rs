use std::{fs, path::Path};

#[test]
fn vendors_are_complete_plain_trees_with_sources_and_licenses() {
    let kit = Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/kit");
    assert!(!kit.join(".gitmodules").exists());
    for (name, source) in [
        (
            "taste-skill",
            "https://github.com/Leonxlnx/taste-skill ce26fc25c0e5e8cab638f883de62d9a86ee5e45b\n",
        ),
        (
            "emil-skills",
            "https://github.com/emilkowalski/skills e8a175de22ae1e49370fc144c1f3bb9aeedf988d\n",
        ),
    ] {
        let vendor = kit.join("vendor").join(name);
        assert!(vendor.join("LICENSE").is_file());
        assert_eq!(fs::read_to_string(vendor.join("SOURCE")).unwrap(), source);
        assert!(!vendor.join(".git").exists());
    }
    assert!(
        kit.join("vendor/taste-skill/skills/taste-skill/SKILL.md")
            .is_file()
    );
    assert!(
        kit.join("vendor/emil-skills/skills/animate/SKILL.md")
            .is_file()
    );
    assert!(
        kit.join("vendor/emil-skills/skills/break-ui/SKILL.md")
            .is_file()
    );
}
