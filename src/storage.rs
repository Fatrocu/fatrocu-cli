use crate::{inference::Endpoint, model::*};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub telemetry_enabled: bool,
    pub default_export: String,
    pub endpoint: Endpoint,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            telemetry_enabled: true,
            default_export: "xlsx".into(),
            endpoint: Endpoint::default(),
        }
    }
}
pub fn default_data_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("FATROCU_DATA_DIR") {
        return Ok(path.into());
    }
    Ok(directories::ProjectDirs::from("org", "Fatrocu", "Fatrocu")
        .context("Cannot locate application data directory")?
        .data_local_dir()
        .to_owned())
}
pub struct Store {
    pub root: PathBuf,
    db: Connection,
}
/// Keep source data and generated financial records out of every Git checkout.
pub fn outside_repository(path: &Path) -> Result<()> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut existing = absolute.as_path();
    while !existing.exists() {
        existing = existing.parent().context("Cannot resolve data path")?;
    }
    let canonical = existing.canonicalize()?;
    ensure!(
        !canonical.ancestors().any(|p| p.join(".git").exists()),
        "Private documents, records, backups and exports must stay outside Git repositories"
    );
    Ok(())
}
impl Store {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_owned();
        outside_repository(&root)?;
        std::fs::create_dir_all(root.join("sources"))?;
        let db = Connection::open(root.join("fatrocu.sqlite3"))?;
        ensure!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))?
                <= crate::SCHEMA_VERSION,
            "Local data uses a newer schema; upgrade the application"
        );
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;
          CREATE TABLE IF NOT EXISTS profiles(id TEXT PRIMARY KEY,json TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS templates(id TEXT PRIMARY KEY,json TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS settings(id INTEGER PRIMARY KEY CHECK(id=1),json TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS documents(id TEXT PRIMARY KEY,profile_id TEXT NOT NULL REFERENCES profiles(id),revision INTEGER NOT NULL,json TEXT NOT NULL,source TEXT NOT NULL);
          PRAGMA user_version=1;")?;
        let store = Self { root, db };
        for kind in [DocumentKind::EArchive, DocumentKind::ZReport] {
            let t = Template::builtin(kind);
            store.db.execute(
                "INSERT OR IGNORE INTO templates VALUES (?1,?2)",
                params![t.id, serde_json::to_string(&t)?],
            )?;
        }
        let _source_lock = store.source_lock()?;
        store.recover_deletions()?;
        Ok(store)
    }
    pub fn profiles(&self) -> Result<Vec<Profile>> {
        self.read_all("SELECT json FROM profiles ORDER BY id")
    }
    pub fn profile(&self, id: &str) -> Result<Profile> {
        self.read_one("SELECT json FROM profiles WHERE id=?1", id)
    }
    pub fn save_profile(&self, p: &Profile) -> Result<()> {
        Profile::new(p.legal_name.clone(), p.tax_id.clone())?;
        let old: Option<String> = self
            .db
            .query_row("SELECT json FROM profiles WHERE id=?1", [&p.id], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(old) = old {
            let old: Profile = serde_json::from_str(&old)?;
            ensure!(
                old.tax_id == p.tax_id || !self.documents()?.iter().any(|d| d.profile_id == p.id),
                "Create a new profile to change a tax ID already used by documents"
            );
        }
        self.db.execute(
            "INSERT INTO profiles VALUES (?1,?2) ON CONFLICT(id) DO UPDATE SET json=excluded.json",
            params![p.id, serde_json::to_string(p)?],
        )?;
        Ok(())
    }
    pub fn delete_profile(&self, id: &str) -> Result<()> {
        self.db
            .execute("DELETE FROM profiles WHERE id=?1", [id])
            .context("A profile used by documents cannot be deleted")?;
        Ok(())
    }
    pub fn templates(&self) -> Result<Vec<Template>> {
        self.read_all("SELECT json FROM templates ORDER BY id")
    }
    pub fn template(&self, id: &str) -> Result<Template> {
        self.read_one("SELECT json FROM templates WHERE id=?1", id)
    }
    pub fn save_template(&self, t: &Template) -> Result<()> {
        t.validate()?;
        ensure!(
            !t.builtin && !matches!(t.id.as_str(), "okc" | "e_archive"),
            "Built-in templates cannot be replaced"
        );
        self.db.execute(
            "INSERT INTO templates VALUES (?1,?2) ON CONFLICT(id) DO UPDATE SET json=excluded.json",
            params![t.id, serde_json::to_string(t)?],
        )?;
        Ok(())
    }
    pub fn settings(&self) -> Result<Settings> {
        let s: Option<String> = self
            .db
            .query_row("SELECT json FROM settings WHERE id=1", [], |r| r.get(0))
            .optional()?;
        Ok(s.map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default())
    }
    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        ensure!(
            matches!(s.default_export.as_str(), "xlsx" | "csv" | "json"),
            "Unsupported export format"
        );
        s.endpoint.validate()?;
        self.db.execute(
            "INSERT INTO settings VALUES (1,?1) ON CONFLICT(id) DO UPDATE SET json=excluded.json",
            [serde_json::to_string(s)?],
        )?;
        Ok(())
    }
    pub fn documents(&self) -> Result<Vec<Document>> {
        self.read_all("SELECT json FROM documents ORDER BY rowid DESC")
    }
    pub fn document(&self, id: &str) -> Result<Document> {
        self.read_one("SELECT json FROM documents WHERE id=?1", id)
    }
    pub fn source(&self, id: &str) -> Result<PathBuf> {
        let s: String =
            self.db
                .query_row("SELECT source FROM documents WHERE id=?1", [id], |r| {
                    r.get(0)
                })?;
        ensure!(
            Path::new(&s).components().count() == 1 && !Path::new(&s).is_absolute(),
            "Invalid stored source path"
        );
        Ok(self.root.join("sources").join(s))
    }
    pub fn import(&self, source: &Path, mut d: Document) -> Result<Document> {
        let _source_lock = self.source_lock()?;
        ensure!(
            d.revision == 0 && d.status == ReviewStatus::Review,
            "Import requires a new review record"
        );
        uuid::Uuid::parse_str(&d.id)?;
        let profile = self.profile(&d.profile_id)?;
        let template = self.template(&d.template_id)?;
        ensure!(
            d.schema_version == crate::SCHEMA_VERSION
                && d.approved_at.is_none()
                && d.accepted_missing.is_empty(),
            "Import requires a current, unapproved schema record"
        );
        ensure!(
            (d.kind == DocumentKind::EArchive) == (template.family == DocumentKind::EArchive)
                && d.fields.keys().eq(template.fields.keys()),
            "Imported fields and kind must match the selected template"
        );
        d.direction(&profile);
        for field in d.fields.values_mut().chain(
            d.taxes
                .iter_mut()
                .flat_map(|t| [&mut t.tax_type, &mut t.rate, &mut t.base, &mut t.amount]),
        ) {
            if field.text().trim().is_empty() {
                *field = Field::new(None);
            }
        }
        let extension = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        ensure!(
            matches!(
                extension.as_str(),
                "pdf" | "png" | "jpg" | "jpeg" | "xml" | "html" | "htm" | "txt"
            ),
            "Unsupported source type"
        );
        let bytes = std::fs::read(source)?;
        ensure!(
            d.source_sha256.is_empty() || d.source_sha256 == hex::encode(Sha256::digest(&bytes)),
            "Source changed after extraction; re-import the document"
        );
        ensure!(
            !bytes.is_empty() && bytes.len() <= 100 * 1024 * 1024,
            "Source must be between 1 byte and 100 MiB"
        );
        d.file_name = source
            .file_name()
            .context("Missing filename")?
            .to_string_lossy()
            .into();
        d.source_sha256 = hex::encode(Sha256::digest(&bytes));
        d.revision = 1;
        let name = format!("{}.{}", d.id, extension);
        let dest = self.root.join("sources").join(&name);
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        if let Err(err) = self.db.execute(
            "INSERT INTO documents VALUES (?1,?2,?3,?4,?5)",
            params![
                d.id,
                d.profile_id,
                d.revision,
                serde_json::to_string(&d)?,
                name
            ],
        ) {
            let _ = std::fs::remove_file(dest);
            return Err(err.into());
        }
        Ok(d)
    }
    fn save(&self, d: &mut Document) -> Result<()> {
        let mut next = d.clone();
        next.revision += 1;
        ensure!(
            self.db.execute(
                "UPDATE documents SET revision=?1,json=?2 WHERE id=?3 AND revision=?4",
                params![
                    next.revision,
                    serde_json::to_string(&next)?,
                    d.id,
                    d.revision
                ]
            )? == 1,
            "Document changed in another session; reload before saving"
        );
        d.revision = next.revision;
        Ok(())
    }
    pub fn review(&self, d: &mut Document) -> Result<()> {
        let old = self.document(&d.id)?;
        validate_review_edit(&old, d)?;
        ensure!(
            d.fields.keys().eq(old.fields.keys()),
            "Review must retain template fields"
        );
        d.accepted_missing.clear();
        d.direction(&self.profile(&d.profile_id)?);
        self.save(d)
    }
    pub fn approve(&self, id: &str, revision: u64, ack: &[String]) -> Result<Document> {
        let mut d = self.document(id)?;
        ensure!(revision == d.revision, "Stale review revision");
        d.approve(ack)?;
        self.save(&mut d)?;
        Ok(d)
    }
    pub fn reopen(&self, id: &str, revision: u64) -> Result<Document> {
        let mut d = self.document(id)?;
        ensure!(d.revision == revision, "Stale document revision");
        d.reopen();
        self.save(&mut d)?;
        Ok(d)
    }
    pub fn delete(&mut self, id: &str, revision: u64) -> Result<()> {
        let _source_lock = self.source_lock()?;
        let source = self.source(id)?;
        let tx = self.db.transaction()?;
        ensure!(
            tx.execute(
                "DELETE FROM documents WHERE id=?1 AND revision=?2",
                params![id, revision]
            )? == 1,
            "Stale document revision"
        );
        if source.exists() {
            let tombstone = source.with_file_name(format!(
                ".{}.deleting",
                source
                    .file_name()
                    .context("Invalid managed source")?
                    .to_string_lossy()
            ));
            ensure!(
                !tombstone.exists(),
                "Interrupted deletion needs recovery before retrying"
            );
            std::fs::rename(&source, &tombstone)?;
            if let Err(error) = tx.commit() {
                std::fs::rename(&tombstone, &source).context(
                    "Deletion rollback failed; retained source is in the sources directory",
                )?;
                return Err(error.into());
            }
            std::fs::remove_file(tombstone)?;
        } else {
            tx.commit()?;
        }
        Ok(())
    }
    fn recover_deletions(&self) -> Result<()> {
        for entry in std::fs::read_dir(self.root.join("sources"))? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(original) = name
                .to_str()
                .and_then(|s| s.strip_prefix('.'))
                .and_then(|s| s.strip_suffix(".deleting"))
            else {
                continue;
            };
            let stem = Path::new(original)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            ensure!(
                uuid::Uuid::parse_str(stem).is_ok(),
                "Unrecognized deletion recovery file; inspect local sources"
            );
            let retained: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM documents WHERE source=?1)",
                [original],
                |r| r.get(0),
            )?;
            if retained {
                let source = self.root.join("sources").join(original);
                ensure!(
                    !source.exists(),
                    "Conflicting deletion recovery files; inspect local sources"
                );
                std::fs::rename(entry.path(), source)?;
            } else {
                std::fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    }
    pub fn backup(&self, destination: &Path) -> Result<()> {
        let _source_lock = self.source_lock()?;
        outside_repository(destination)?;
        ensure!(!destination.exists(), "Backup destination must not exist");
        std::fs::create_dir_all(destination.join("sources"))?;
        self.db
            .backup("main", destination.join("fatrocu.sqlite3"), None)?;
        for e in std::fs::read_dir(self.root.join("sources"))? {
            let e = e?;
            if e.file_type()?.is_file() {
                std::fs::copy(e.path(), destination.join("sources").join(e.file_name()))?;
            }
        }
        Ok(())
    }
    fn read_all<T: serde::de::DeserializeOwned>(&self, sql: &str) -> Result<Vec<T>> {
        let mut st = self.db.prepare(sql)?;
        let values = st
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        values
            .iter()
            .map(|s| Ok(serde_json::from_str(s)?))
            .collect()
    }
    fn source_lock(&self) -> Result<std::fs::File> {
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("sources.lock"))?;
        lock.lock()?;
        Ok(lock)
    }
    fn read_one<T: serde::de::DeserializeOwned>(&self, sql: &str, id: &str) -> Result<T> {
        Ok(serde_json::from_str(
            &self
                .db
                .query_row(sql, [id], |r| r.get::<_, String>(0))
                .context("Record not found")?,
        )?)
    }
}
