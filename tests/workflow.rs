use fatrocu_core::{
    benchmark::{self, GroundTruth, Score},
    export::{self, ExportFormat},
    extraction::{self, parse_ubl},
    inference::Endpoint,
    model::*,
    storage::Store,
    update,
};
use std::{
    io::{Read, Write},
    net::TcpListener,
};
fn profile() -> Profile {
    {
        let mut p = Profile::new("Example Taxpayer".into(), "1234567890".into()).unwrap();
        p.id = "00000000-0000-4000-8000-000000000001".into();
        p
    }
}
fn document(kind: DocumentKind) -> Document {
    Document::blank(&profile(), &Template::builtin(kind), kind)
}
fn field(v: &str) -> Field {
    Field::new(Some(v.into()))
}
fn tax(rate: &str, base: &str, amount: &str) -> TaxRow {
    TaxRow {
        tax_type: field("VAT"),
        rate: field(rate),
        base: field(base),
        amount: field(amount),
    }
}
fn complete() -> Document {
    let mut d = document(DocumentKind::EArchive);
    for (k, v) in [
        ("document_number", "TEST-001"),
        ("date", "2026-01-03"),
        ("seller_tax_id", "1234567890"),
        ("seller_name", "Example Taxpayer"),
        ("buyer_tax_id", "9876543210"),
        ("buyer_name", "Example Buyer"),
        ("gross_total", "120.00"),
        ("currency", "TRY"),
    ] {
        d.set(k, Some(v.into()));
    }
    d.direction(&profile());
    d.taxes = vec![tax("20", "100", "20")];
    d
}

#[test]
fn missing_values_remain_null_and_need_explicit_acknowledgement() {
    let mut d = document(DocumentKind::ZReport);
    assert!(
        d.fields
            .values()
            .all(|f| f.value.is_none() && f.needs_review)
    );
    assert!(d.approve(&[]).is_err());
    let paths = d
        .issues()
        .iter()
        .map(|i| i.path.clone())
        .collect::<Vec<_>>();
    d.approve(&paths).unwrap();
    assert!(
        d.fields
            .values()
            .all(|f| f.value.is_none() && f.needs_review)
    );
}
#[test]
fn direction_never_guesses() {
    let mut d = complete();
    assert_eq!(d.value("direction"), "sale");
    d.set("seller_tax_id", Some("5555555555".into()));
    d.direction(&profile());
    assert_eq!(d.value("direction"), "");
    d.set("buyer_tax_id", Some(profile().tax_id));
    d.direction(&profile());
    assert_eq!(d.value("direction"), "purchase");
    d.set("seller_tax_id", Some(profile().tax_id));
    d.direction(&profile());
    assert_eq!(d.value("direction"), "");
}
#[test]
fn z_reports_do_not_require_plate_but_utts_does() {
    assert!(
        !document(DocumentKind::ZReport)
            .required()
            .contains(&"vehicle_plate")
    );
    assert!(
        document(DocumentKind::Utts)
            .required()
            .contains(&"vehicle_plate")
    );
}
#[test]
fn tax_merge_is_exact_and_keeps_incomplete_rows_separate() {
    let rows = vec![
        tax("20", "0.10", "0.02"),
        tax("20.0", "0.20", "0.04"),
        tax("10", "1.00", "0.10"),
        TaxRow {
            base: Field::new(None),
            ..tax("20", "2", "0.4")
        },
    ];
    let merged = merge_taxes(&rows).unwrap();
    assert_eq!(merged.len(), 3);
    assert_eq!(merged[0].base.text(), "0.3");
    assert_eq!(merged[0].amount.text(), "0.06");
    assert_eq!(rows.len(), 4);
    assert!(merged[2].base.value.is_none());
}
#[test]
fn invalid_values_cannot_be_acknowledged() {
    let mut d = complete();
    d.set("date", Some("2026-02-30".into()));
    let paths = d
        .issues()
        .iter()
        .map(|i| i.path.clone())
        .collect::<Vec<_>>();
    assert!(d.approve(&paths).is_err());
}
#[test]
fn ubl_does_not_double_count_line_tax_or_use_an_unrelated_id() {
    let xml = r#"<Invoice xmlns:c="urn:example"><ID>TEST-UBL</ID><IssueDate>2026-01-03</IssueDate><DocumentCurrencyCode>TRY</DocumentCurrencyCode><AccountingSupplierParty><Party><PartyIdentification><ID schemeID="OTHER">1111111111</ID></PartyIdentification><PartyIdentification><ID schemeID="VKN">1234567890</ID></PartyIdentification><PartyName><Name>Example Seller</Name></PartyName></Party></AccountingSupplierParty><AccountingCustomerParty><Party><PartyIdentification><ID schemeID="TCKN">12345678901</ID></PartyIdentification><Person><FirstName>Example</FirstName><FamilyName>Buyer</FamilyName></Person></Party></AccountingCustomerParty><TaxTotal><TaxSubtotal><TaxableAmount>100.00</TaxableAmount><TaxAmount>20.00</TaxAmount><Percent>20</Percent><TaxCategory><TaxScheme><TaxTypeCode>0015</TaxTypeCode></TaxScheme></TaxCategory></TaxSubtotal></TaxTotal><LegalMonetaryTotal><PayableAmount>120.00</PayableAmount></LegalMonetaryTotal><InvoiceLine><TaxTotal><TaxSubtotal><TaxAmount>20</TaxAmount></TaxSubtotal></TaxTotal></InvoiceLine></Invoice>"#;
    let mut d = document(DocumentKind::EArchive);
    parse_ubl(xml, &mut d).unwrap();
    assert_eq!(d.taxes.len(), 1);
    assert_eq!(d.value("seller_tax_id"), "1234567890");
    assert_eq!(d.value("buyer_name"), "Example Buyer");
    assert_eq!(d.taxes[0].tax_type.text(), "VAT");
    assert_eq!(d.value("gross_total"), "120.00");
}
#[test]
fn storage_round_trip_revision_approval_reopen_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path()).unwrap();
    s.save_profile(&profile()).unwrap();
    let source = dir.path().join("synthetic.txt");
    std::fs::write(&source, "Synthetic test source").unwrap();
    let mut d = s.import(&source, complete()).unwrap();
    assert_eq!(d.revision, 1);
    let stale = d.clone();
    d.set("seller_name", Some("Corrected Example".into()));
    s.review(&mut d).unwrap();
    assert!(s.review(&mut stale.clone()).is_err());
    let approved = s.approve(&d.id, d.revision, &[]).unwrap();
    assert!(s.review(&mut approved.clone()).is_err());
    let reopened = s.reopen(&approved.id, approved.revision).unwrap();
    assert_eq!(reopened.status, ReviewStatus::Review);
    let retained = s.source(&d.id).unwrap();
    assert!(retained.is_file());
    let backup = dir.path().join("backup");
    s.backup(&backup).unwrap();
    let other = Store::open(&backup).unwrap();
    assert_eq!(other.documents().unwrap().len(), 1);
    s.delete(&reopened.id, reopened.revision).unwrap();
    assert!(!retained.exists());
    assert!(s.documents().unwrap().is_empty());
    assert!(other.source(&d.id).unwrap().is_file());
}
#[test]
fn template_and_profile_changes_cannot_corrupt_existing_records() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path()).unwrap();
    let p = profile();
    s.save_profile(&p).unwrap();
    let file = dir.path().join("test.txt");
    std::fs::write(&file, "Synthetic").unwrap();
    let mut d = complete();
    d.profile_id = p.id.clone();
    s.import(&file, d).unwrap();
    let mut changed = p.clone();
    changed.tax_id = "9876543210".into();
    assert!(s.save_profile(&changed).is_err());
    assert!(s.delete_profile(&p.id).is_err());
    assert!(
        s.save_template(&Template::builtin(DocumentKind::EArchive))
            .is_err()
    );
}
#[test]
fn exports_require_approval_and_produce_real_xlsx() {
    let dir = tempfile::tempdir().unwrap();
    let mut d = complete();
    assert!(
        export::export(
            &[d.clone()],
            ExportFormat::Json,
            &dir.path().join("no.json")
        )
        .is_err()
    );
    d.approve(&[]).unwrap();
    for (ext, format) in [
        ("xlsx", ExportFormat::Xlsx),
        ("csv", ExportFormat::Csv),
        ("json", ExportFormat::Json),
    ] {
        let path = dir.path().join(format!("export.{ext}"));
        export::export(&[d.clone()], format, &path).unwrap();
        if ext == "xlsx" {
            let bytes = std::fs::read(&path).unwrap();
            assert!(bytes.starts_with(b"PK"));
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
            assert!(zip.by_name("xl/worksheets/sheet2.xml").is_ok());
        }
        assert!(export::export(&[d.clone()], format, &path).is_err());
    }
}
#[test]
fn csv_does_not_execute_untrusted_formulas() {
    let dir = tempfile::tempdir().unwrap();
    let mut d = complete();
    d.set("seller_name", Some("=HYPERLINK(\"test\")".into()));
    d.approve(&[]).unwrap();
    let path = dir.path().join("safe.csv");
    export::export(&[d], ExportFormat::Csv, &path).unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains("'=HYPERLINK"));
}
#[test]
fn remote_document_transfer_is_blocked_before_network() {
    for url in [
        "https://example.invalid/v1",
        "http://localhost:1842/v1",
        "http://192.0.2.1/v1",
    ] {
        let e = Endpoint {
            url: url.into(),
            ..Endpoint::default()
        };
        assert!(e.is_remote());
        assert!(e.authorize(false).is_err());
    }
    for url in ["http://127.0.0.1:1842/v1", "http://[::1]:1842/v1"] {
        let e = Endpoint {
            url: url.into(),
            ..Endpoint::default()
        };
        assert!(!e.is_remote());
        assert!(e.authorize(false).is_ok());
    }
}
#[test]
fn model_response_normalizes_nulls_and_preserves_schema() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = vec![0; 65536];
        let _ = stream.read(&mut request).unwrap();
        let content=serde_json::json!({"fields":{"document_number":{"value":null,"needs_review":false},"seller_name":{"value":"Example Seller","source":{"page":1,"x":2,"y":0,"width":0.1,"height":0.1}}},"taxes":[]}).to_string();
        let body = serde_json::json!({"choices":[{"message":{"content":content}}]}).to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    let endpoint = Endpoint {
        url: format!("http://{address}/v1"),
        ..Endpoint::default()
    };
    let p = endpoint
        .infer(
            &Template::builtin(DocumentKind::EArchive),
            DocumentKind::EArchive,
            Some("Synthetic test text"),
            &[],
            false,
        )
        .unwrap();
    assert!(p.fields["document_number"].needs_review);
    assert!(p.fields["seller_name"].source.is_none());
    assert!(p.fields["seller_name"].needs_review);
    server.join().unwrap();
}
#[test]
fn benchmark_scores_each_field_and_unordered_duplicate_tax_rows() {
    let mut actual = complete();
    actual.taxes = vec![tax("20", "100", "20"), tax("20", "200", "40")];
    let truth = GroundTruth {
        kind: actual.kind,
        profile: profile(),
        fields: actual.fields.clone(),
        taxes: actual.taxes.iter().rev().cloned().collect(),
    };
    let mut score = Score::default();
    benchmark::score_case(&mut score, &truth, &actual, false).unwrap();
    assert_eq!(score.exact_documents.correct, 1);
    assert_eq!(score.required.correct, score.required.total);
    actual.set("document_number", Some("TEST-002".into()));
    let mut score = Score::default();
    benchmark::score_case(&mut score, &truth, &actual, false).unwrap();
    assert_eq!(score.exact_documents.correct, 0);
    assert!(score.critical.correct < score.critical.total);
}
#[test]
fn benchmark_critical_null_hallucinations_fail() {
    let mut d = complete();
    let mut fields = d.fields.clone();
    fields.insert("document_number".into(), Field::new(None));
    let truth = GroundTruth {
        kind: d.kind,
        profile: profile(),
        fields,
        taxes: d.taxes.clone(),
    };
    let mut score = Score::default();
    benchmark::score_case(&mut score, &truth, &d, true).unwrap();
    assert_eq!(score.critical_hallucinations, 1);
    d.set("document_number", None);
    let mut score = Score::default();
    benchmark::score_case(&mut score, &truth, &d, true).unwrap();
    assert_eq!(score.exact_documents.correct, 1);
}
#[test]
fn benchmark_thresholds_are_not_rounded() {
    let count = benchmark::Count {
        correct: 979,
        total: 1000,
    };
    assert!(!count.passes(98));
}
#[test]
fn deterministic_import_does_not_contact_a_model() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("z.txt");
    std::fs::write(&file,"Z no: TEST-Z\nDate: 03.01.2026\nSeller tax ID: 1234567890\nSeller name: Example Seller\nGross total: 120,00\nTax: VAT;20;100;20").unwrap();
    let d = extraction::extract(
        &file,
        &profile(),
        &Template::builtin(DocumentKind::ZReport),
        DocumentKind::ZReport,
        &Endpoint {
            url: "https://example.invalid/v1".into(),
            ..Endpoint::default()
        },
        false,
        true,
    )
    .unwrap();
    assert_eq!(d.value("document_number"), "TEST-Z");
    assert_eq!(d.value("date"), "2026-01-03");
    assert!(d.model.is_none());
}
#[test]
fn digest_and_stable_semantic_release_checks_fail_closed() {
    use sha2::{Digest, Sha256};
    let bytes = b"synthetic release";
    assert!(
        update::verify_digest(
            bytes,
            &format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
        )
        .is_ok()
    );
    assert!(update::verify_digest(bytes, &format!("sha256:{}", "0".repeat(64))).is_err());
    let mut r = update::Release {
        tag_name: "v3.10.0".into(),
        draft: false,
        prerelease: false,
        body: None,
        html_url: "https://github.com/Fatrocu/Fatrocu/releases/tag/v3.10.0".into(),
        assets: vec![],
    };
    assert!(update::newer(&r, "3.9.0").unwrap());
    r.prerelease = true;
    assert!(update::newer(&r, "3.9.0").is_err());
}
#[test]
fn review_cannot_replace_document_identity() {
    let old = complete();
    let mut edit = old.clone();
    edit.source_sha256 = "different".into();
    assert!(validate_review_edit(&old, &edit).is_err());
}
#[test]
fn telemetry_preference_does_not_create_or_purge_a_package() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path()).unwrap();
    assert!(s.settings().unwrap().telemetry_enabled);
    let existing = dir.path().join("telemetry-existing");
    std::fs::write(&existing, "Synthetic local data").unwrap();
    let mut settings = s.settings().unwrap();
    settings.telemetry_enabled = false;
    s.save_settings(&settings).unwrap();
    assert!(!s.settings().unwrap().telemetry_enabled);
    assert!(existing.exists());
}

#[test]
fn source_hash_changes_are_rejected_and_deletions_recover_after_interruption() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("synthetic.txt");
    std::fs::write(&source, "Z no: SYNTHETIC-Z").unwrap();
    let p = profile();
    let s = Store::open(temp.path().join("data")).unwrap();
    s.save_profile(&p).unwrap();
    let extracted = extraction::extract(
        &source,
        &p,
        &Template::builtin(DocumentKind::ZReport),
        DocumentKind::ZReport,
        &Endpoint::default(),
        false,
        false,
    )
    .unwrap();
    std::fs::write(&source, "Changed synthetic source").unwrap();
    assert!(s.import(&source, extracted).is_err());
    let d = s.import(&source, complete()).unwrap();
    let retained = s.source(&d.id).unwrap();
    let tombstone = retained.with_file_name(format!(
        ".{}.deleting",
        retained.file_name().unwrap().to_string_lossy()
    ));
    std::fs::rename(&retained, &tombstone).unwrap();
    drop(s);
    let reopened = Store::open(temp.path().join("data")).unwrap();
    assert!(reopened.source(&d.id).unwrap().is_file());
    assert!(!tombstone.exists());
}
#[test]
fn private_storage_and_exports_are_rejected_inside_git_checkouts() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join(".git")).unwrap();
    assert!(Store::open(temp.path().join("data")).is_err());
    let mut d = complete();
    d.approve(&[]).unwrap();
    assert!(export::export(&[d], ExportFormat::Json, &temp.path().join("private.json")).is_err());
}
#[test]
fn spurious_tax_rows_are_critical_hallucinations() {
    let d = complete();
    let truth = GroundTruth {
        kind: d.kind,
        profile: profile(),
        fields: d.fields.clone(),
        taxes: vec![],
    };
    let mut score = Score::default();
    benchmark::score_case(&mut score, &truth, &d, false).unwrap();
    assert_eq!(score.critical_hallucinations, 3);
    assert_eq!(score.exact_documents.correct, 0);
}
#[test]
fn claude_messages_protocol_uses_the_document_free_mock_endpoint() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = vec![0; 65536];
        let n = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..n]);
        assert!(request.starts_with("POST /v1/messages "));
        assert!(
            request
                .to_lowercase()
                .contains("anthropic-version: 2023-06-01")
        );
        let response =
            serde_json::json!({"content":[{"type":"text","text":"{\"fields\":{},\"taxes\":[]}"}]})
                .to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
    });
    let endpoint = Endpoint {
        url: format!("http://{address}/v1"),
        protocol: fatrocu_core::inference::Protocol::Anthropic,
        ..Default::default()
    };
    assert!(
        endpoint
            .infer(
                &Template::builtin(DocumentKind::EArchive),
                DocumentKind::EArchive,
                Some("Synthetic test data"),
                &[],
                false
            )
            .unwrap()
            .fields
            .is_empty()
    );
    server.join().unwrap();
}
