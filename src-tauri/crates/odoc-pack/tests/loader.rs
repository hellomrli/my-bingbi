use odoc_model::DocumentType;
use odoc_pack::{LoadedPack, PACK_SCHEMA_VERSION, PackError, PackRegistry, ResourceRole, StageId};
use std::fs;
use std::path::{Path, PathBuf};

fn repository_pack() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../packs/gb9704-format")
}

fn write_valid_pack(root: &Path, id: &str, binding_resource: &str, rules: &str) {
    fs::create_dir_all(root.join("rules")).expect("rules directory should be created");
    fs::write(
        root.join("pack.toml"),
        format!(
            r#"schema_version = "{PACK_SCHEMA_VERSION}"

[pack]
id = "{id}"
name = "测试包"
version = "0.1.0"
description = "测试声明式知识包"
license = "MIT"

[capabilities]
provides = ["format_rules"]

[[bindings]]
stage = "lint"
rules = ["{binding_resource}"]
"#
        ),
    )
    .expect("manifest should be written");
    fs::write(root.join(binding_resource), rules).expect("rules should be written");
}

const VALID_RULE: &str = r"- id: valid-rule
  name: 有效规则
  severity: warn
  type: regex
  scope: body.title.text
  pattern: '^ok$'
  match: required
  message: 规则有效
";

#[test]
fn loads_repository_pack_and_resolves_declared_resources() {
    let pack = LoadedPack::load(repository_pack()).expect("repository pack should load");
    assert_eq!(pack.manifest().pack.id, "gb9704-format");
    let rules = pack
        .resources_for_stage(StageId::Lint, None)
        .expect("lint resources should resolve");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].relative_path, "rules/format.yaml");
    assert_eq!(rules[0].role, ResourceRole::Rule);
    let style = pack
        .resources_for_stage(StageId::Export, None)
        .expect("style resources should resolve");
    assert_eq!(style[0].relative_path, "style/gb9704.toml");
    assert!(
        pack.read_text("rules/format.yaml")
            .unwrap()
            .contains("wenhao-bracket")
    );
    let pack_path = repository_pack();
    let registry =
        PackRegistry::load(pack_path.parent().unwrap()).expect("repository registry should load");
    assert_eq!(registry.len(), 2);
    assert!(registry.get("gb9704-format").is_some());
    assert!(registry.get("renmin-style").is_some());

    for doc_type in [
        DocumentType::Request,
        DocumentType::Notice,
        DocumentType::Report,
        DocumentType::Letter,
        DocumentType::Minutes,
        DocumentType::Summary,
        DocumentType::Decision,
        DocumentType::Announcement,
    ] {
        let resource = pack
            .resources_for_stage(StageId::Outline, Some(doc_type))
            .expect("template should resolve")
            .into_iter()
            .next()
            .expect("template should exist");
        let template = pack
            .read_template(&resource.relative_path)
            .expect("template should parse");
        assert_eq!(template.doc_type, doc_type);
        assert!(!template.blocks.is_empty());
    }
}

#[test]
fn rejects_manifest_directory_mismatch_and_invalid_schema() {
    let temp = tempfile::tempdir().expect("temporary directory should exist");
    let wrong = temp.path().join("wrong-directory");
    write_valid_pack(&wrong, "declared-id", "rules/main.yaml", VALID_RULE);
    assert!(matches!(
        LoadedPack::load(&wrong),
        Err(PackError::DirectoryIdMismatch(_))
    ));

    let invalid = temp.path().join("invalid-schema");
    write_valid_pack(&invalid, "invalid-schema", "rules/main.yaml", VALID_RULE);
    let manifest = fs::read_to_string(invalid.join("pack.toml")).unwrap();
    fs::write(
        invalid.join("pack.toml"),
        manifest.replace(PACK_SCHEMA_VERSION, "9.9"),
    )
    .unwrap();
    assert!(matches!(
        LoadedPack::load(&invalid),
        Err(PackError::UnsupportedSchema(_))
    ));
}

#[test]
fn rejects_path_traversal_unknown_placeholders_and_bad_extensions() {
    let temp = tempfile::tempdir().expect("temporary directory should exist");
    let traversal = temp.path().join("traversal");
    write_valid_pack(&traversal, "traversal", "../outside.yaml", VALID_RULE);
    assert!(matches!(
        LoadedPack::load(&traversal),
        Err(PackError::InvalidResourcePath(_))
    ));

    let placeholder = temp.path().join("placeholder");
    write_valid_pack(
        &placeholder,
        "placeholder",
        "rules/{unknown}.yaml",
        VALID_RULE,
    );
    assert!(matches!(
        LoadedPack::load(&placeholder),
        Err(PackError::InvalidResourcePath(_))
    ));

    let extension = temp.path().join("extension");
    write_valid_pack(&extension, "extension", "rules/main.txt", VALID_RULE);
    assert!(matches!(
        LoadedPack::load(&extension),
        Err(PackError::UnsupportedResource(_))
    ));
}

#[test]
fn rejects_duplicate_rule_ids_and_invalid_rule_yaml() {
    let temp = tempfile::tempdir().expect("temporary directory should exist");
    let duplicate = temp.path().join("duplicate");
    write_valid_pack(
        &duplicate,
        "duplicate",
        "rules/main.yaml",
        &format!("{VALID_RULE}\n{VALID_RULE}"),
    );
    assert!(matches!(
        LoadedPack::load(&duplicate),
        Err(PackError::DuplicateRuleId(_))
    ));

    let invalid = temp.path().join("bad-yaml");
    write_valid_pack(&invalid, "bad-yaml", "rules/main.yaml", "not: a rule list");
    assert!(matches!(
        LoadedPack::load(&invalid),
        Err(PackError::InvalidYaml(_))
    ));
}

#[test]
fn rejects_symbolic_links_and_oversized_files() {
    let temp = tempfile::tempdir().expect("temporary directory should exist");
    let target = temp.path().join("target.yaml");
    fs::write(&target, VALID_RULE).unwrap();

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, temp.path().join("linked")).unwrap();
        assert!(matches!(
            LoadedPack::load(temp.path()),
            Err(PackError::SymlinkNotAllowed(_))
        ));
    }

    let oversized = temp.path().join("oversized");
    write_valid_pack(&oversized, "oversized", "rules/main.yaml", VALID_RULE);
    let huge = vec![b'x'; 2 * 1024 * 1024 + 1];
    fs::write(oversized.join("rules/main.yaml"), huge).unwrap();
    assert!(matches!(
        LoadedPack::load(&oversized),
        Err(PackError::FileTooLarge { .. })
    ));
}

#[test]
fn resolves_document_type_placeholders_only_for_declared_types() {
    let temp = tempfile::tempdir().expect("temporary directory should exist");
    let root = temp.path().join("templates");
    fs::create_dir_all(root.join("templates")).unwrap();
    fs::write(
        root.join("pack.toml"),
        format!(
            r#"schema_version = "{PACK_SCHEMA_VERSION}"
[pack]
id = "templates"
name = "模板"
version = "0.1.0"
license = "MIT"
[capabilities]
doc_types = ["request"]
provides = ["templates"]
[[bindings]]
stage = "outline"
inject = ["templates/{{doc_type}}.yaml"]
"#
        ),
    )
    .unwrap();
    fs::write(
        root.join("templates/request.yaml"),
        "doc_type: request\nblocks: []\n",
    )
    .unwrap();
    let pack = LoadedPack::load(&root).expect("template pack should load");
    let resources = pack
        .resources_for_stage(StageId::Outline, Some(DocumentType::Request))
        .unwrap();
    assert_eq!(resources[0].relative_path, "templates/request.yaml");
    assert!(
        pack.resources_for_stage(StageId::Outline, Some(DocumentType::Notice))
            .is_err()
    );
}

#[test]
fn pack_registry_rejects_directory_id_mismatches() {
    let temp = tempfile::tempdir().expect("temporary directory should exist");
    let first = temp.path().join("first");
    write_valid_pack(&first, "different", "rules/main.yaml", VALID_RULE);
    let error = PackRegistry::load(temp.path()).expect_err("mismatched ID should fail");
    assert!(matches!(error, PackError::DirectoryIdMismatch(_)));
}

#[test]
fn read_text_rejects_files_that_are_not_declared_resources() {
    let pack = LoadedPack::load(repository_pack()).expect("repository pack should load");
    let error = pack
        .read_text("SKILL.md")
        .expect_err("undeclared skill file should not be readable");
    assert!(matches!(error, PackError::UndeclaredResource(path) if path == "SKILL.md"));
}

#[test]
fn rejects_invalid_rule_parameters() {
    let cases = [
        (
            "bad-regex",
            "- id: bad-regex\n  name: 非法正则\n  severity: warn\n  type: regex\n  scope: body.blocks\n  pattern: '['\n  match: forbidden\n  message: 非法正则\n",
        ),
        (
            "missing-match",
            "- id: missing-match\n  name: 缺少匹配模式\n  severity: warn\n  type: regex\n  scope: body.blocks\n  pattern: ok\n  message: 缺少匹配模式\n",
        ),
        (
            "bad-density",
            "- id: bad-density\n  name: 非法密度\n  severity: warn\n  type: lexicon_density\n  scope: body.blocks\n  lexicon: [口号]\n  max_per_100_chars: -1\n  message: 非法密度\n",
        ),
        (
            "bad-structure",
            "- id: bad-structure\n  name: 非法结构参数\n  severity: warn\n  type: structure\n  scope: body.blocks\n  detect: parallel_phrases\n  min_run: 1\n  message: 非法结构参数\n",
        ),
    ];

    for (id, rules) in cases {
        let temp = tempfile::tempdir().expect("temporary directory should exist");
        let root = temp.path().join(id);
        write_valid_pack(&root, id, "rules/main.yaml", rules);
        assert!(
            matches!(LoadedPack::load(root), Err(PackError::InvalidRule { .. })),
            "{id} should be rejected"
        );
    }
}
