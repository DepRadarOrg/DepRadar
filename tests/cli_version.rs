use assert_cmd::Command;

#[test]
fn version_includes_semver_and_build_commit() {
    let output = Command::cargo_bin("depradar")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let version = String::from_utf8(output).unwrap();
    let version = version.trim();
    let prefix = format!("depradar {} (", env!("CARGO_PKG_VERSION"));
    assert!(
        version.starts_with(&prefix),
        "unexpected version output: {version}"
    );
    assert!(
        version.ends_with(')'),
        "unexpected version output: {version}"
    );
    let sha = &version[prefix.len()..version.len() - 1];
    assert!(
        sha == "unknown" || (sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())),
        "unexpected build identifier: {sha}"
    );
}
