use fatrocu_core::{extraction, inference::Endpoint, model::*};
fn pdf(path: &std::path::Path) {
    let lines = [
        "Z no: SYNTHETIC-Z",
        "Date: 2026-01-03",
        "Seller tax ID: 1234567890",
        "Seller name: Synthetic Seller",
        "Gross total: 120.00",
        "Tax: VAT;20;100;20",
    ];
    let mut stream = String::from("BT /F1 12 Tf 50 780 Td 16 TL ");
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            stream.push_str(" T* ");
        }
        stream.push_str(&format!("({line}) Tj"));
    }
    stream.push_str(" ET");
    let objects=["<< /Type /Catalog /Pages 2 0 R >>".into(),"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".into(),"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),format!("<< /Length {} >>\nstream\n{}\nendstream",stream.len(),stream)];
    let mut output = String::from("%PDF-1.4\n");
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(output.len());
        output.push_str(&format!("{} 0 obj\n{object}\nendobj\n", i + 1));
    }
    let start = output.len();
    output.push_str("xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets.into_iter().skip(1) {
        output.push_str(&format!("{offset:010} 00000 n \n"));
    }
    output.push_str(&format!(
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n"
    ));
    std::fs::write(path, output).unwrap();
}
#[test]
fn text_pdf_is_deterministic_without_model_access() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.pdf");
    pdf(&path);
    let profile = Profile::new("Synthetic Seller".into(), "1234567890".into()).unwrap();
    let d = extraction::extract(
        &path,
        &profile,
        &Template::builtin(DocumentKind::ZReport),
        DocumentKind::ZReport,
        &Endpoint {
            url: "https://example.invalid/v1".into(),
            ..Default::default()
        },
        false,
        true,
    )
    .unwrap();
    assert_eq!(d.value("document_number"), "SYNTHETIC-Z");
    assert_eq!(d.taxes.len(), 1);
    assert!(d.model.is_none());
    assert!(!d.source_sha256.is_empty());
}
#[test]
#[ignore = "Requires Poppler pdftoppm on PATH; run explicitly during desktop validation"]
fn real_pdf_page_renders_for_review() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("synthetic.pdf");
    pdf(&source);
    let preview = temp.path().join("preview.png");
    extraction::render_page(&source, 1, &preview).unwrap();
    let image = image::open(preview).unwrap();
    assert!(image.width() > 500 && image.height() > 500);
}
