use crate::model::*;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Xlsx,
    Csv,
    Json,
}
impl std::str::FromStr for ExportFormat {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self> {
        Ok(match value {
            "xlsx" => Self::Xlsx,
            "csv" => Self::Csv,
            "json" => Self::Json,
            _ => anyhow::bail!("Unsupported export format"),
        })
    }
}
pub fn export(documents: &[Document], format: ExportFormat, path: &Path) -> Result<()> {
    crate::storage::outside_repository(path)?;
    ensure!(
        !documents.is_empty(),
        "Select at least one approved document"
    );
    ensure!(
        documents.iter().all(|d| d.status == ReviewStatus::Approved),
        "Only approved documents can be exported"
    );
    ensure!(
        !path.exists(),
        "Export destination exists; choose a new file"
    );
    let mut output = tempfile::NamedTempFile::new_in(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    let keys: BTreeSet<_> = documents
        .iter()
        .flat_map(|d| d.fields.keys().cloned())
        .collect();
    let mut headers = vec!["id".to_owned(), "profile_id".into(), "document_type".into()];
    headers.extend(keys.iter().cloned());
    let doc_row = |d: &Document| {
        let mut row = vec![
            d.id.clone(),
            d.profile_id.clone(),
            match d.kind {
                DocumentKind::EArchive => "e_archive",
                DocumentKind::ZReport => "z_report",
                DocumentKind::Utts => "utts",
            }
            .into(),
        ];
        row.extend(keys.iter().map(|k| d.value(k).to_owned()));
        row
    };
    match format {
        ExportFormat::Json => {
            let mut docs = documents.to_vec();
            for d in &mut docs {
                d.taxes = d.tax_view()?;
            }
            serde_json::to_writer_pretty(output.as_file_mut(), &docs)?;
        }
        ExportFormat::Csv => {
            let mut writer = csv::Writer::from_writer(output.as_file_mut());
            let mut header = headers.clone();
            header.extend(["tax_type", "tax_rate", "tax_base", "tax_amount"].map(str::to_owned));
            writer.write_record(header)?;
            for d in documents {
                let taxes = d.tax_view()?;
                if taxes.is_empty() {
                    let mut row = doc_row(d);
                    row.extend([String::new(), String::new(), String::new(), String::new()]);
                    writer.write_record(row.into_iter().map(csv_safe))?;
                } else {
                    for t in taxes {
                        let mut row = doc_row(d);
                        row.extend(
                            [t.tax_type, t.rate, t.base, t.amount].map(|f| f.text().to_owned()),
                        );
                        writer.write_record(row.into_iter().map(csv_safe))?;
                    }
                }
            }
            writer.flush()?;
        }
        ExportFormat::Xlsx => {
            let mut book = rust_xlsxwriter::Workbook::new();
            let header_format = rust_xlsxwriter::Format::new().set_bold();
            let sheet = book.add_worksheet();
            sheet.set_name("Documents")?;
            sheet.set_freeze_panes(1, 0)?;
            for (col, key) in headers.iter().enumerate() {
                sheet.write_string_with_format(0, col as u16, key, &header_format)?;
                sheet.set_column_width(col as u16, 22)?;
            }
            for (i, d) in documents.iter().enumerate() {
                for (j, value) in doc_row(d).iter().enumerate() {
                    sheet.write_string(i as u32 + 1, j as u16, value)?;
                }
            }
            let sheet = book.add_worksheet();
            sheet.set_name("Taxes")?;
            sheet.set_freeze_panes(1, 0)?;
            for (i, name) in ["document_id", "tax_type", "rate", "base", "amount"]
                .iter()
                .enumerate()
            {
                sheet.write_string_with_format(0, i as u16, *name, &header_format)?;
                sheet.set_column_width(i as u16, 24)?;
            }
            let mut i = 1;
            for d in documents {
                for tax in d.tax_view()? {
                    sheet.write_string(i, 0, &d.id)?;
                    for (j, f) in [&tax.tax_type, &tax.rate, &tax.base, &tax.amount]
                        .iter()
                        .enumerate()
                    {
                        sheet.write_string(i, j as u16 + 1, f.text())?;
                    }
                    i += 1;
                }
            }
            book.save(output.path())?;
        }
    }
    output.as_file().sync_all()?;
    output.persist_noclobber(path)?;
    Ok(())
}
/// Prevent spreadsheet formulas from being executed when opening untrusted source values.
fn csv_safe(value: String) -> String {
    if value
        .trim_start()
        .starts_with(['=', '+', '-', '@', '\t', '\r'])
    {
        format!("'{value}")
    } else {
        value
    }
}
