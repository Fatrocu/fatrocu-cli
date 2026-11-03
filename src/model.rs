use anyhow::{Context, Result, bail, ensure};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    EArchive,
    ZReport,
    Utts,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Region {
    /// One-based source page; normalized top-left coordinates.
    pub page: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Region {
    pub fn valid(&self) -> bool {
        self.page > 0
            && [self.x, self.y, self.width, self.height]
                .iter()
                .all(|x| x.is_finite() && *x >= 0.0)
            && self.width > 0.0
            && self.height > 0.0
            && self.x + self.width <= 1.001
            && self.y + self.height <= 1.001
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Field {
    pub value: Option<String>,
    pub source: Option<Region>,
    pub needs_review: bool,
}
impl Field {
    pub fn new(value: Option<String>) -> Self {
        let value = value.filter(|s| !s.trim().is_empty());
        Self {
            needs_review: value.is_none(),
            value,
            source: None,
        }
    }
    pub fn text(&self) -> &str {
        self.value.as_deref().unwrap_or("")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct TaxRow {
    pub tax_type: Field,
    pub rate: Field,
    pub base: Field,
    pub amount: Field,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub id: String,
    pub legal_name: String,
    pub tax_id: String,
}
impl Profile {
    pub fn new(legal_name: String, tax_id: String) -> Result<Self> {
        ensure!(!legal_name.trim().is_empty(), "Legal name is required");
        ensure!(valid_tax_id(&tax_id), "Tax ID must contain 10 or 11 digits");
        Ok(Self {
            id: Uuid::new_v4().to_string(),
            legal_name,
            tax_id,
        })
    }
}
pub fn valid_tax_id(s: &str) -> bool {
    matches!(s.len(), 10 | 11) && s.bytes().all(|b| b.is_ascii_digit())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub family: DocumentKind,
    pub fields: BTreeMap<String, String>,
    pub builtin: bool,
}
impl Template {
    pub fn builtin(kind: DocumentKind) -> Self {
        let (id, name) = match kind {
            DocumentKind::EArchive => ("e_archive", "e-Archive"),
            _ => ("okc", "OKC / Z report / UTTS"),
        };
        let mut fields: BTreeMap<_, _> = [
            ("document_number", "Document number"),
            ("date", "Date"),
            ("seller_tax_id", "Seller VKN/TCKN"),
            ("seller_name", "Seller legal name"),
            ("gross_total", "Gross total"),
            ("currency", "Currency"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
        if kind == DocumentKind::EArchive {
            fields.extend([
                ("buyer_tax_id".into(), "Buyer VKN/TCKN".into()),
                ("buyer_name".into(), "Buyer legal name".into()),
                ("direction".into(), "Purchase / sale".into()),
            ]);
        } else {
            fields.insert("vehicle_plate".into(), "Vehicle plate (UTTS only)".into());
        }
        Self {
            id: id.into(),
            name: name.into(),
            family: if kind == DocumentKind::EArchive {
                kind
            } else {
                DocumentKind::ZReport
            },
            fields,
            builtin: true,
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.id.trim().is_empty() && !self.name.trim().is_empty(),
            "Template ID and name are required"
        );
        let base = Self::builtin(self.family);
        ensure!(
            base.fields.keys().all(|k| self.fields.contains_key(k)),
            "Custom templates must retain the built-in fields"
        );
        ensure!(
            self.fields.iter().all(|(k, v)| !k.is_empty()
                && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && !v.trim().is_empty()),
            "Use ASCII field keys and nonempty labels"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Review,
    Approved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Document {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub profile_id: String,
    pub template_id: String,
    pub kind: DocumentKind,
    pub file_name: String,
    pub source_sha256: String,
    pub created_at: String,
    pub status: ReviewStatus,
    pub fields: BTreeMap<String, Field>,
    /// Unmerged source rows survive toggling the merged view.
    pub taxes: Vec<TaxRow>,
    pub merge_taxes: bool,
    pub accepted_missing: BTreeSet<String>,
    pub extraction_method: String,
    pub model: Option<String>,
    pub approved_at: Option<String>,
}
impl Document {
    pub fn blank(profile: &Profile, template: &Template, kind: DocumentKind) -> Self {
        Self {
            schema_version: crate::SCHEMA_VERSION,
            id: Uuid::new_v4().to_string(),
            revision: 0,
            profile_id: profile.id.clone(),
            template_id: template.id.clone(),
            kind,
            file_name: String::new(),
            source_sha256: String::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
            status: ReviewStatus::Review,
            fields: template
                .fields
                .keys()
                .map(|k| (k.clone(), Field::new(None)))
                .collect(),
            taxes: vec![],
            merge_taxes: true,
            accepted_missing: BTreeSet::new(),
            extraction_method: "manual".into(),
            model: None,
            approved_at: None,
        }
    }
    pub fn value(&self, key: &str) -> &str {
        self.fields.get(key).map(Field::text).unwrap_or("")
    }
    pub fn set(&mut self, key: &str, value: Option<String>) {
        self.fields.insert(key.into(), Field::new(value));
        self.accepted_missing.remove(key);
    }
    pub fn direction(&mut self, profile: &Profile) {
        if self.kind != DocumentKind::EArchive {
            return;
        }
        self.set(
            "direction",
            match (
                self.value("seller_tax_id") == profile.tax_id,
                self.value("buyer_tax_id") == profile.tax_id,
            ) {
                (true, false) => Some("sale".into()),
                (false, true) => Some("purchase".into()),
                _ => None,
            },
        );
    }
    pub fn required(&self) -> Vec<&str> {
        let mut fields = vec![
            "document_number",
            "date",
            "seller_tax_id",
            "seller_name",
            "gross_total",
        ];
        match self.kind {
            DocumentKind::EArchive => fields.extend(["buyer_tax_id", "buyer_name", "direction"]),
            DocumentKind::Utts => fields.push("vehicle_plate"),
            _ => {}
        }
        fields
    }
    pub fn issues(&self) -> Vec<Issue> {
        let mut issues = vec![];
        for key in self.required() {
            let f = self.fields.get(key);
            if f.is_none_or(|f| f.text().trim().is_empty()) {
                issues.push(Issue::missing(
                    key,
                    "Missing or unreadable; correct or explicitly acknowledge",
                ));
            } else if f.is_some_and(|f| f.needs_review) {
                issues.push(Issue::missing(key, "Value requires human review"));
            }
        }
        for (key, field) in &self.fields {
            if field.source.as_ref().is_some_and(|s| !s.valid()) {
                issues.push(Issue::invalid(key, "Invalid source coordinates"));
            }
        }
        if self.taxes.is_empty() {
            issues.push(Issue::missing(
                "taxes",
                "No tax rows extracted; confirm the source has no readable tax table",
            ));
        }
        for (i, row) in self.taxes.iter().enumerate() {
            for (key, field) in [
                ("tax_type", &row.tax_type),
                ("rate", &row.rate),
                ("base", &row.base),
                ("amount", &row.amount),
            ] {
                let path = format!("taxes.{i}.{key}");
                if field.text().is_empty() || field.needs_review {
                    issues.push(Issue::missing(&path, "Missing or unreviewed tax field"));
                }
                if key != "tax_type" && !field.text().is_empty() && decimal(field.text()).is_err() {
                    issues.push(Issue::invalid(&path, "Invalid decimal"));
                }
                if field.source.as_ref().is_some_and(|s| !s.valid()) {
                    issues.push(Issue::invalid(&path, "Invalid source coordinates"));
                }
            }
        }
        for key in ["seller_tax_id", "buyer_tax_id"] {
            if !self.value(key).is_empty() && !valid_tax_id(self.value(key)) {
                issues.push(Issue::invalid(key, "Tax ID must contain 10 or 11 digits"));
            }
        }
        if !self.value("date").is_empty()
            && NaiveDate::parse_from_str(self.value("date"), "%Y-%m-%d").is_err()
        {
            issues.push(Issue::invalid("date", "Use a valid YYYY-MM-DD date"));
        }
        if !self.value("gross_total").is_empty() && decimal(self.value("gross_total")).is_err() {
            issues.push(Issue::invalid("gross_total", "Invalid gross total"));
        }
        if !self.value("direction").is_empty()
            && !matches!(self.value("direction"), "purchase" | "sale")
        {
            issues.push(Issue::invalid("direction", "Use purchase or sale"));
        }
        issues
    }
    pub fn approve(&mut self, acknowledged: &[String]) -> Result<()> {
        ensure!(
            self.status == ReviewStatus::Review,
            "Send the document back to review before editing"
        );
        let issues = self.issues();
        for path in acknowledged {
            ensure!(
                issues.iter().any(|i| i.path == *path && i.acknowledgeable),
                "Cannot acknowledge an invalid or unknown issue: {path}"
            );
        }
        let accepted: BTreeSet<_> = self
            .accepted_missing
            .iter()
            .chain(acknowledged)
            .cloned()
            .collect();
        let unresolved: Vec<_> = issues
            .iter()
            .filter(|i| !i.acknowledgeable || !accepted.contains(&i.path))
            .collect();
        ensure!(
            unresolved.is_empty(),
            "Resolve review issues before approval: {}",
            unresolved
                .iter()
                .map(|i| i.path.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        self.accepted_missing = accepted;
        self.status = ReviewStatus::Approved;
        self.approved_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
    pub fn reopen(&mut self) {
        self.status = ReviewStatus::Review;
        self.approved_at = None;
        self.accepted_missing.clear();
    }
    pub fn tax_view(&self) -> Result<Vec<TaxRow>> {
        if self.merge_taxes {
            merge_taxes(&self.taxes)
        } else {
            Ok(self.taxes.clone())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub path: String,
    pub message: String,
    pub acknowledgeable: bool,
}
impl Issue {
    fn missing(path: &str, message: &str) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
            acknowledgeable: true,
        }
    }
    fn invalid(path: &str, message: &str) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
            acknowledgeable: false,
        }
    }
}

/// Normalize separators without approximating or changing digits.
pub fn decimal(value: &str) -> Result<Decimal> {
    let value = value.trim().replace(['\u{00a0}', ' '], "");
    let normalized = if value.contains(',') {
        value.replace('.', "").replace(',', ".")
    } else {
        value
    };
    ensure!(!normalized.is_empty(), "Empty decimal");
    Decimal::from_str_exact(&normalized).context("Invalid decimal")
}
pub fn merge_taxes(rows: &[TaxRow]) -> Result<Vec<TaxRow>> {
    let mut result: Vec<TaxRow> = vec![];
    for row in rows {
        if [&row.tax_type, &row.rate, &row.base, &row.amount]
            .iter()
            .any(|f| f.text().is_empty() || f.needs_review)
        {
            result.push(row.clone());
            continue;
        }
        let rate = decimal(row.rate.text())?;
        if let Some(existing) = result.iter_mut().find(|r| {
            r.tax_type.text() == row.tax_type.text()
                && decimal(r.rate.text()).ok() == Some(rate)
                && !r.base.text().is_empty()
                && !r.amount.text().is_empty()
                && !r.base.needs_review
                && !r.amount.needs_review
                && !r.tax_type.needs_review
                && !r.rate.needs_review
        }) {
            existing.base = Field::new(Some(
                (decimal(existing.base.text())? + decimal(row.base.text())?)
                    .normalize()
                    .to_string(),
            ));
            existing.amount = Field::new(Some(
                (decimal(existing.amount.text())? + decimal(row.amount.text())?)
                    .normalize()
                    .to_string(),
            ));
        } else {
            result.push(row.clone());
        }
    }
    Ok(result)
}
pub fn validate_review_edit(original: &Document, edited: &Document) -> Result<()> {
    ensure!(
        original.status == ReviewStatus::Review,
        "Approved records must be reopened before editing"
    );
    ensure!(
        edited.id == original.id
            && edited.revision == original.revision
            && edited.profile_id == original.profile_id
            && edited.template_id == original.template_id
            && edited.kind == original.kind
            && edited.source_sha256 == original.source_sha256
            && edited.file_name == original.file_name
            && edited.schema_version == original.schema_version
            && edited.created_at == original.created_at
            && edited.extraction_method == original.extraction_method
            && edited.model == original.model,
        "Review cannot change source identity or extraction provenance"
    );
    if edited.status != ReviewStatus::Review || edited.approved_at.is_some() {
        bail!("Use approval to approve a document");
    }
    Ok(())
}
