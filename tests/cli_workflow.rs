use serde_json::Value;
use std::{path::Path, process::Command};
fn cli(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fatrocu-cli"))
        .arg("--data-dir")
        .arg(root)
        .args(args)
        .env("FATROCU_NO_UPDATE_CHECK", "1")
        .output()
        .unwrap()
}
fn json(root: &Path, args: &[&str]) -> Value {
    let out = cli(root, args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn automation_import_review_approve_export_and_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("data");
    let p = json(
        &root,
        &[
            "profile",
            "add",
            "--name",
            "Synthetic Test Company",
            "--tax-id",
            "1234567890",
        ],
    );
    let id = p["id"].as_str().unwrap();
    let source = temp.path().join("synthetic.txt");
    std::fs::write(&source,"Invoice number: SYNTHETIC-001\nDate: 2026-01-03\nSeller tax ID: 1234567890\nSeller name: Synthetic Test Company\nBuyer tax ID: 9876543210\nBuyer name: Synthetic Buyer\nGross total: 120.00\nCurrency: TRY\nTax: VAT;20;100;20").unwrap();
    let result = json(
        &root,
        &[
            "process",
            source.to_str().unwrap(),
            "--profile",
            id,
            "--no-model",
        ],
    );
    let doc_id = result[0]["document"]["id"].as_str().unwrap();
    assert_eq!(
        result[0]["document"]["fields"]["direction"]["value"],
        "sale"
    );
    let review = json(
        &root,
        &[
            "review",
            doc_id,
            "--revision",
            "1",
            "--set",
            "seller_name=Synthetic Corrected Company",
        ],
    );
    assert_eq!(review["revision"], 2);
    assert!(
        !cli(&root, &["approve", doc_id, "--revision", "1"])
            .status
            .success()
    );
    let approved = json(&root, &["approve", doc_id, "--revision", "2"]);
    assert_eq!(approved["status"], "approved");
    for ext in ["xlsx", "csv", "json"] {
        let path = temp.path().join(format!("export.{ext}"));
        json(
            &root,
            &[
                "export",
                "--output",
                path.to_str().unwrap(),
                "--format",
                ext,
                "--id",
                doc_id,
            ],
        );
        assert!(path.is_file());
    }
    let reopened = json(&root, &["reopen", doc_id, "--revision", "3"]);
    assert_eq!(reopened["status"], "review");
    assert!(
        !cli(
            &root,
            &[
                "export",
                "--output",
                temp.path().join("unapproved.json").to_str().unwrap(),
                "--format",
                "json",
                "--id",
                doc_id
            ]
        )
        .status
        .success()
    );
}
#[test]
fn benchmark_stays_disabled_without_readiness() {
    let temp = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fatrocu-cli"))
        .arg("--data-dir")
        .arg(temp.path().join("data"))
        .args(["benchmark", "--dataset"])
        .arg(temp.path().join("assets"))
        .arg("--report")
        .arg(temp.path().join("report.json"))
        .args(["--commit", "test"])
        .env_remove("FATROCU_BENCHMARK_ENABLED")
        .env("FATROCU_NO_UPDATE_CHECK", "1")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Benchmark is on hold"));
    assert!(!temp.path().join("report.json").exists());
}
