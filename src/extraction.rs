use crate::{inference::Endpoint, model::*};
use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Run a helper without a console window and bound its execution time.
pub fn run_helper(command: &mut Command) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().context(
        "Cannot start document helper; install Poppler and add its bin directory to PATH",
    )?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), "Document helper failed");
            return Ok(());
        }
        if start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("Document helper timed out");
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}
pub fn render_page(source: &Path, page: u32, destination: &Path) -> Result<()> {
    ensure!(page > 0, "Pages are one-based");
    let prefix = destination.with_extension("");
    run_helper(
        Command::new("pdftoppm")
            .args([
                "-png",
                "-singlefile",
                "-scale-to",
                "2000",
                "-f",
                &page.to_string(),
                "-l",
                &page.to_string(),
            ])
            .arg(source)
            .arg(&prefix),
    )?;
    let result = prefix.with_extension("png");
    if result != destination {
        std::fs::rename(result, destination)?;
    }
    Ok(())
}
pub fn page_count(source: &Path) -> Result<u32> {
    if source
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        Ok(pdf_extract::Document::load(source)?.get_pages().len() as u32)
    } else {
        Ok(1)
    }
}
pub fn extract(
    source: &Path,
    profile: &Profile,
    template: &Template,
    kind: DocumentKind,
    endpoint: &Endpoint,
    allow_remote: bool,
    use_model: bool,
) -> Result<Document> {
    ensure!(
        (kind == DocumentKind::EArchive) == (template.family == DocumentKind::EArchive),
        "Document kind does not match template family"
    );
    let metadata = std::fs::metadata(source)?;
    use sha2::{Digest, Sha256};
    let source_hash = hex::encode(Sha256::digest(std::fs::read(source)?));
    ensure!(
        metadata.len() > 0 && metadata.len() <= 100 * 1024 * 1024,
        "Source must be between 1 byte and 100 MiB"
    );
    let ext = source
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut document = Document::blank(profile, template, kind);
    let mut text = None;
    let mut images = vec![];
    let scratch = tempfile::tempdir()?;
    let mut pdf_pages = 0;
    match ext.as_str() {
        "xml" => {
            let source = std::fs::read_to_string(source)?;
            parse_ubl(&source, &mut document)?;
            text = Some(source);
        }
        "html" | "htm" => {
            let source = std::fs::read_to_string(source)?;
            let html = scraper::Html::parse_document(&source);
            for element in html.select(&scraper::Selector::parse("[data-field]").unwrap()) {
                if let Some(key) = element.value().attr("data-field")
                    && template.fields.contains_key(key)
                {
                    document.set(key, Some(element.text().collect::<String>().trim().into()));
                }
            }
            for row in html.select(&scraper::Selector::parse("tr").unwrap()) {
                let cells: Vec<_> = row
                    .select(&scraper::Selector::parse("td,th").unwrap())
                    .map(|c| c.text().collect::<String>())
                    .collect();
                if cells.len() == 2 {
                    parse_labeled_text(
                        &format!(
                            "{}: {}",
                            cells[0].trim().trim_end_matches(':'),
                            cells[1].trim()
                        ),
                        &mut document,
                    );
                }
            }
            text = Some(html.root_element().text().collect::<Vec<_>>().join("\n"));
            document.extraction_method = "html".into();
        }
        "txt" => text = Some(std::fs::read_to_string(source)?),
        "pdf" => {
            let bbox = scratch.path().join("text.xml");
            pdf_pages = pdf_extract::Document::load(source)?.get_pages().len();
            ensure!(
                pdf_pages > 0 && pdf_pages <= 50,
                "PDF must contain 1 to 50 pages"
            );
            let has_bbox = run_helper(
                Command::new("pdftotext")
                    .args(["-bbox-layout", "-enc", "UTF-8"])
                    .arg(source)
                    .arg(&bbox),
            )
            .is_ok();
            if has_bbox {
                let raw = std::fs::read_to_string(bbox)?;
                let parsed = roxmltree::Document::parse(&raw)?;
                let content = parsed
                    .descendants()
                    .filter(|n| n.has_tag_name("line"))
                    .map(|line| {
                        line.descendants()
                            .filter(|n| n.has_tag_name("word"))
                            .filter_map(|n| n.text())
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if !content.trim().is_empty() {
                    text = Some(content);
                }
                // Bbox words remain available for exact, unique evidence matching below.
                std::fs::write(scratch.path().join("bbox.xml"), raw)?;
            } else {
                let content =
                    pdf_extract::extract_text(source).context("Cannot read PDF text layer")?;
                if !content.trim().is_empty() {
                    text = Some(content);
                }
            }
        }
        "png" | "jpg" | "jpeg" => images.push(source.to_owned()),
        _ => anyhow::bail!("Unsupported document type: {ext}"),
    }
    if let Some(text) = &text {
        parse_labeled_text(text, &mut document);
    }
    document.direction(profile);
    let complete = document.issues().is_empty();
    if use_model && !complete {
        endpoint.authorize(allow_remote)?;
        if ext == "pdf" {
            // Render every page, including pages containing scans mixed with text.
            for page in 1..=pdf_pages {
                let path = scratch.path().join(format!("page-{page}.png"));
                render_page(source, page as u32, &path)?;
                images.push(path);
            }
        }
        let prediction = endpoint.infer(template, kind, text.as_deref(), &images, allow_remote)?;
        for (key, field) in prediction.fields {
            if document.value(&key).is_empty() {
                document.fields.insert(key, field);
            }
        }
        if document.taxes.is_empty() {
            document.taxes = prediction.taxes;
        }
        document.extraction_method = if document.extraction_method == "manual" {
            "model".into()
        } else {
            format!("{}+model", document.extraction_method)
        };
        document.model = Some(endpoint.model.clone());
    }
    if ext == "pdf" && scratch.path().join("bbox.xml").exists() {
        attach_bbox(
            &std::fs::read_to_string(scratch.path().join("bbox.xml"))?,
            &mut document,
        )?;
    }
    document.direction(profile);
    let pages = if ext == "pdf" { pdf_pages as u32 } else { 1 };
    for field in document.fields.values_mut().chain(
        document
            .taxes
            .iter_mut()
            .flat_map(|t| [&mut t.tax_type, &mut t.rate, &mut t.base, &mut t.amount]),
    ) {
        if field.source.as_ref().is_some_and(|r| r.page > pages) {
            field.source = None;
            field.needs_review = true;
        }
    }
    ensure!(
        source_hash == hex::encode(Sha256::digest(std::fs::read(source)?)),
        "Source changed during extraction; retry import"
    );
    document.source_sha256 = source_hash;
    Ok(document)
}

fn child<'a, 'i>(node: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
}
fn value(node: roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    child(node, name)
        .and_then(|n| n.text())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}
pub fn parse_ubl(xml: &str, d: &mut Document) -> Result<()> {
    let xml = roxmltree::Document::parse(xml).context("Invalid XML document")?;
    let root = xml.root_element();
    ensure!(
        root.tag_name().name() == "Invoice",
        "Expected a UBL Invoice"
    );
    d.set("document_number", value(root, "ID"));
    d.set("date", value(root, "IssueDate"));
    d.set("currency", value(root, "DocumentCurrencyCode"));
    for (tag, prefix) in [
        ("AccountingSupplierParty", "seller"),
        ("AccountingCustomerParty", "buyer"),
    ] {
        if let Some(party) = child(root, tag).and_then(|n| child(n, "Party")) {
            let ids: Vec<_> = party
                .descendants()
                .filter(|n| {
                    n.has_tag_name("ID") && matches!(n.attribute("schemeID"), Some("VKN" | "TCKN"))
                })
                .filter_map(|n| n.text())
                .collect();
            let unique: std::collections::BTreeSet<_> = ids.into_iter().collect();
            if unique.len() == 1 {
                d.set(
                    &format!("{prefix}_tax_id"),
                    unique.first().map(|s| s.to_string()),
                );
            }
            let name = child(party, "PartyLegalEntity")
                .and_then(|n| value(n, "RegistrationName"))
                .or_else(|| child(party, "PartyName").and_then(|n| value(n, "Name")))
                .or_else(|| {
                    child(party, "Person").and_then(|n| {
                        Some(format!(
                            "{} {}",
                            value(n, "FirstName")?,
                            value(n, "FamilyName")?
                        ))
                    })
                });
            d.set(&format!("{prefix}_name"), name);
        }
    }
    d.set(
        "gross_total",
        child(root, "LegalMonetaryTotal")
            .and_then(|n| value(n, "TaxInclusiveAmount").or_else(|| value(n, "PayableAmount"))),
    );
    for total in root
        .children()
        .filter(|n| n.has_tag_name("TaxTotal") || n.has_tag_name("WithholdingTaxTotal"))
    {
        for subtotal in total.children().filter(|n| n.has_tag_name("TaxSubtotal")) {
            let category = child(subtotal, "TaxCategory");
            let scheme = category.and_then(|n| child(n, "TaxScheme"));
            let code = scheme.and_then(|n| value(n, "TaxTypeCode"));
            let tax_type = if code.as_deref() == Some("0015") {
                Some("VAT".into())
            } else {
                scheme.and_then(|n| value(n, "Name")).or(code)
            };
            d.taxes.push(TaxRow {
                tax_type: Field::new(tax_type),
                rate: Field::new(
                    value(subtotal, "Percent")
                        .or_else(|| category.and_then(|n| value(n, "Percent"))),
                ),
                base: Field::new(value(subtotal, "TaxableAmount")),
                amount: Field::new(value(subtotal, "TaxAmount")),
            });
        }
    }
    d.extraction_method = "ubl".into();
    Ok(())
}

pub fn parse_labeled_text(text: &str, d: &mut Document) {
    // Only explicit label/value lines are deterministic. Free-layout OCR uses the model.
    for line in text.lines() {
        let Some((label, value)) = line.split_once(':') else {
            continue;
        };
        let label = label.trim().to_lowercase();
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let key = match label.as_str() {
            "invoice number" | "fatura no" | "fatura numarası"
                if d.kind == DocumentKind::EArchive =>
            {
                Some("document_number")
            }
            "z no" | "z number" if d.kind != DocumentKind::EArchive => Some("document_number"),
            "date" | "tarih" => Some("date"),
            "seller tax id" | "satıcı vkn" => Some("seller_tax_id"),
            "seller name" | "satıcı unvan" => Some("seller_name"),
            "buyer tax id" | "alıcı vkn" => Some("buyer_tax_id"),
            "buyer name" | "alıcı unvan" => Some("buyer_name"),
            "gross total" | "genel toplam" => Some("gross_total"),
            "vehicle plate" | "plaka" => Some("vehicle_plate"),
            "currency" => Some("currency"),
            _ => None,
        };
        if let Some(key) = key
            && d.fields.contains_key(key)
            && d.value(key).is_empty()
        {
            let v = if key == "date" {
                normalize_date(value)
            } else {
                value.to_owned()
            };
            d.set(key, Some(v));
        }
        // Explicit machine-readable tax lines: Tax: type;rate;base;amount.
        if label == "tax" {
            let cells: Vec<_> = value.split(';').map(str::trim).collect();
            if cells.len() == 4 {
                d.taxes.push(TaxRow {
                    tax_type: Field::new(Some(cells[0].into())),
                    rate: Field::new(Some(cells[1].into())),
                    base: Field::new(Some(cells[2].into())),
                    amount: Field::new(Some(cells[3].into())),
                });
            }
        }
    }
    if d.extraction_method == "manual" && d.fields.values().any(|f| f.value.is_some()) {
        d.extraction_method = "labeled_text".into();
    }
}
pub fn normalize_date(value: &str) -> String {
    for format in ["%Y-%m-%d", "%d.%m.%Y", "%d/%m/%Y"] {
        if let Ok(d) = chrono::NaiveDate::parse_from_str(value, format) {
            return d.format("%Y-%m-%d").to_string();
        }
    }
    value.into()
}
fn attach_bbox(raw: &str, d: &mut Document) -> Result<()> {
    let xml = roxmltree::Document::parse(raw)?;
    for field in d.fields.values_mut().chain(
        d.taxes
            .iter_mut()
            .flat_map(|t| [&mut t.tax_type, &mut t.rate, &mut t.base, &mut t.amount]),
    ) {
        let mut matches = vec![];
        for (i, page) in xml
            .descendants()
            .filter(|n| n.has_tag_name("page"))
            .enumerate()
        {
            let width = page
                .attribute("width")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.0);
            let height = page
                .attribute("height")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.0);
            for word in page.descendants().filter(|n| {
                n.has_tag_name("word") && n.text() == field.value.as_deref() && n.text().is_some()
            }) {
                let attr = |name| word.attribute(name).and_then(|v| v.parse::<f32>().ok());
                if let (Some(x), Some(y), Some(x2), Some(y2)) =
                    (attr("xMin"), attr("yMin"), attr("xMax"), attr("yMax"))
                {
                    let r = Region {
                        page: i as u32 + 1,
                        x: x / width,
                        y: y / height,
                        width: (x2 - x) / width,
                        height: (y2 - y) / height,
                    };
                    if r.valid() {
                        matches.push(r);
                    }
                }
            }
        }
        if matches.len() == 1 {
            field.source = matches.pop();
        }
    }
    Ok(())
}
pub fn preview_paths(source: &Path) -> Result<Vec<PathBuf>> {
    Ok(vec![source.to_owned()])
}
