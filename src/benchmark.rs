//! Private local accuracy gate. Reports contain counts and field names only.
use crate::{extraction::extract, inference::Endpoint, model::*};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use unicode_normalization::UnicodeNormalization;

#[derive(Deserialize)]
pub struct Manifest {
    pub version: String,
    pub model_revision: String,
    pub documents: Vec<Case>,
}
#[derive(Deserialize)]
pub struct Case {
    pub source: String,
    pub ground_truth: String,
}
#[derive(Deserialize)]
pub struct GroundTruth {
    pub kind: DocumentKind,
    pub profile: Profile,
    pub fields: BTreeMap<String, Field>,
    pub taxes: Vec<TaxRow>,
}
#[derive(Default, Debug, Clone, Serialize)]
pub struct Count {
    pub correct: u64,
    pub total: u64,
}
impl Count {
    fn add(&mut self, ok: bool) {
        self.total += 1;
        self.correct += u64::from(ok);
    }
    pub fn passes(&self, percent: u64) -> bool {
        self.total > 0 && self.correct * 100 >= self.total * percent
    }
}
#[derive(Default, Serialize)]
pub struct Score {
    pub documents: u64,
    pub required: Count,
    pub critical: Count,
    pub exact_documents: Count,
    pub critical_hallucinations: u64,
    pub fields: BTreeMap<String, Count>,
    pub passed: bool,
}
#[derive(Serialize)]
pub struct Report {
    pub commit: String,
    pub model_id: String,
    pub model_revision: String,
    pub schema_version: u32,
    pub dataset_version: String,
    pub manifest_sha256: String,
    pub modes: BTreeMap<String, BTreeMap<String, Score>>,
    pub passed: bool,
}

fn normalize(key: &str, s: &str) -> String {
    if matches!(key, "gross_total" | "rate" | "base" | "amount") {
        return decimal(s)
            .map(|n| n.normalize().to_string())
            .unwrap_or_else(|_| s.into());
    }
    if key == "date" {
        return crate::extraction::normalize_date(s);
    }
    let text = s
        .nfc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if matches!(key, "direction" | "tax_type" | "currency" | "vehicle_plate") {
        text.to_lowercase()
    } else {
        text
    }
}
fn equal(key: &str, expected: &Field, actual: Option<&Field>) -> bool {
    let value = actual.map(Field::text).unwrap_or("");
    if expected.text().is_empty() {
        return value.is_empty() && actual.is_some_and(|f| f.needs_review);
    }
    !value.is_empty() && normalize(key, expected.text()) == normalize(key, value)
}
fn critical(key: &str) -> bool {
    matches!(
        key,
        "document_number"
            | "date"
            | "seller_tax_id"
            | "buyer_tax_id"
            | "gross_total"
            | "rate"
            | "base"
            | "amount"
    )
}
fn add(score: &mut Score, key: &str, expected: &Field, actual: Option<&Field>) -> bool {
    let ok = equal(key, expected, actual);
    score.required.add(ok);
    score.fields.entry(key.into()).or_default().add(ok);
    if critical(key) {
        score.critical.add(ok);
        if expected.text().is_empty() && actual.is_some_and(|f| !f.text().is_empty()) {
            score.critical_hallucinations += 1;
        }
    }
    ok
}
fn tax_fields(row: &TaxRow) -> [(&str, &Field); 4] {
    [
        ("tax_type", &row.tax_type),
        ("rate", &row.rate),
        ("base", &row.base),
        ("amount", &row.amount),
    ]
}
pub fn score_case(
    score: &mut Score,
    truth: &GroundTruth,
    actual: &Document,
    merged: bool,
) -> Result<()> {
    score.documents += 1;
    let mut exact = true;
    for (key, expected) in &truth.fields {
        if key == "vehicle_plate" && truth.kind != DocumentKind::Utts {
            continue;
        }
        exact &= add(score, key, expected, actual.fields.get(key));
    }
    let expected = if merged {
        merge_taxes(&truth.taxes)?
    } else {
        truth.taxes.clone()
    };
    let got = if merged {
        merge_taxes(&actual.taxes)?
    } else {
        actual.taxes.clone()
    };
    // Match exact rows first, then maximize matching cells using a minimum-cost assignment.
    let n = expected.len().max(got.len());
    let mut weights = vec![vec![0i64; n]; n];
    for (i, e) in expected.iter().enumerate() {
        for (j, a) in got.iter().enumerate() {
            weights[i][j] = tax_fields(e)
                .iter()
                .zip(tax_fields(a))
                .filter(|((key, e), (_, a))| equal(key, e, Some(a)))
                .count() as i64;
        }
    }
    let assignment = maximum_assignment(&weights);
    for (i, &j) in assignment.iter().enumerate() {
        match (expected.get(i), got.get(j)) {
            (Some(e), Some(a)) => {
                for ((key, e), (_, a)) in tax_fields(e).into_iter().zip(tax_fields(a)) {
                    exact &= add(score, key, e, Some(a));
                }
            }
            (Some(e), None) => {
                for (key, e) in tax_fields(e) {
                    exact &= add(score, key, e, None);
                }
            }
            (None, Some(row)) => {
                score.critical_hallucinations += [&row.rate, &row.base, &row.amount]
                    .iter()
                    .filter(|f| !f.text().is_empty())
                    .count() as u64;
                for key in ["tax_type", "rate", "base", "amount"] {
                    score.required.add(false);
                    score.fields.entry(key.into()).or_default().add(false);
                    if critical(key) {
                        score.critical.add(false);
                    }
                }
                exact = false;
            }
            _ => {}
        }
    }
    score.exact_documents.add(exact);
    Ok(())
}
fn maximum_assignment(weights: &[Vec<i64>]) -> Vec<usize> {
    let n = weights.len();
    let (mut u, mut v, mut p, mut way) = (
        vec![0; n + 1],
        vec![0; n + 1],
        vec![0usize; n + 1],
        vec![0usize; n + 1],
    );
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0;
        let mut min = vec![i64::MAX; n + 1];
        let mut used = vec![false; n + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let mut delta = i64::MAX;
            let mut j1 = 0;
            for j in 1..=n {
                if !used[j] {
                    let cur = 4 - weights[i0 - 1][j - 1] - u[i0] - v[j];
                    if cur < min[j] {
                        min[j] = cur;
                        way[j] = j0;
                    }
                    if min[j] < delta {
                        delta = min[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=n {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    min[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    let mut result = vec![0; n];
    for j in 1..=n {
        result[p[j] - 1] = j - 1;
    }
    result
}
fn private_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let p = root
        .join(relative)
        .canonicalize()
        .context("Dataset input is missing")?;
    ensure!(
        p.starts_with(root) && p.is_file(),
        "Dataset paths must stay inside assets/"
    );
    Ok(p)
}
pub fn run(dataset: &Path, endpoint: &Endpoint, commit: String) -> Result<Report> {
    let env = std::env::var_os("FATROCU_BENCHMARK_DATA")
        .context("Set FATROCU_BENCHMARK_DATA to the private workspace-root assets/ directory")?;
    let root = PathBuf::from(env).canonicalize()?;
    ensure!(
        dataset.canonicalize()? == root && root.file_name().is_some_and(|s| s == "assets"),
        "Benchmark data must come from FATROCU_BENCHMARK_DATA pointing to assets/"
    );
    ensure!(
        !endpoint.is_remote(),
        "Private benchmark uses local inference only"
    );
    let bytes = std::fs::read(root.join("manifest.json"))?;
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    ensure!(
        !manifest.version.is_empty() && !manifest.model_revision.is_empty(),
        "Dataset and model revisions are required"
    );
    let mut cases = vec![];
    let mut counts = [0, 0];
    let mut hashes = std::collections::BTreeSet::new();
    for (index, c) in manifest.documents.iter().enumerate() {
        let source = private_path(&root, &c.source)?;
        let truth: GroundTruth =
            serde_json::from_slice(&std::fs::read(private_path(&root, &c.ground_truth)?)?)
                .with_context(|| format!("Invalid ground truth at case index {index}"))?;
        ensure!(
            hashes.insert(hex::encode(Sha256::digest(std::fs::read(&source)?))),
            "Duplicate source documents cannot count toward the corpus minimum"
        );
        let template = Template::builtin(truth.kind);
        let blank = Document::blank(&truth.profile, &template, truth.kind);
        ensure!(
            blank
                .required()
                .iter()
                .all(|k| truth.fields.contains_key(*k)),
            "Ground truth must label every required scalar field (case {index})"
        );
        for f in truth.fields.values().chain(
            truth
                .taxes
                .iter()
                .flat_map(|r| [&r.tax_type, &r.rate, &r.base, &r.amount]),
        ) {
            ensure!(
                !f.text().is_empty() || f.needs_review,
                "Null labels must require review (case {index})"
            );
        }
        counts[usize::from(truth.kind != DocumentKind::EArchive)] += 1;
        cases.push((source, truth));
    }
    ensure!(
        counts.iter().all(|n| *n >= 10),
        "BLOCKED: need at least 10 distinct human-labeled documents per template (e-Archive: {}, OKC: {})",
        counts[0],
        counts[1]
    );
    let mut modes: BTreeMap<String, BTreeMap<String, Score>> = ["merged", "unmerged"]
        .into_iter()
        .map(|mode| {
            (
                mode.into(),
                ["e_archive", "okc"]
                    .into_iter()
                    .map(|k| (k.into(), Score::default()))
                    .collect(),
            )
        })
        .collect();
    for (source, truth) in cases {
        let actual = extract(
            &source,
            &truth.profile,
            &Template::builtin(truth.kind),
            truth.kind,
            endpoint,
            false,
            true,
        )?;
        let family = if truth.kind == DocumentKind::EArchive {
            "e_archive"
        } else {
            "okc"
        };
        for (mode, scores) in &mut modes {
            score_case(
                scores.get_mut(family).unwrap(),
                &truth,
                &actual,
                mode == "merged",
            )?;
        }
    }
    let mut passed = true;
    for scores in modes.values_mut() {
        for score in scores.values_mut() {
            score.passed = score.required.passes(95)
                && score.critical.passes(98)
                && score.exact_documents.passes(90)
                && score.critical_hallucinations == 0;
            passed &= score.passed;
        }
    }
    Ok(Report {
        commit,
        model_id: endpoint.model.clone(),
        model_revision: manifest.model_revision,
        schema_version: crate::SCHEMA_VERSION,
        dataset_version: manifest.version,
        manifest_sha256: hex::encode(Sha256::digest(bytes)),
        modes,
        passed,
    })
}
