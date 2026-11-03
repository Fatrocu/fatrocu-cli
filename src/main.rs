use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use fatrocu_core::{
    benchmark,
    export::{self, ExportFormat},
    extraction,
    inference::Endpoint,
    model::*,
    storage::{self, Settings, Store},
    update,
};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Parser)]
#[command(
    name = "fatrocu-cli",
    version,
    about = "Local financial document extraction, human review and export"
)]
struct Cli {
    #[arg(long, env = "FATROCU_DATA_DIR", global = true)]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Manage taxpayer profiles. Select a profile explicitly for every import.
    Profile {
        #[command(subcommand)]
        action: Profiles,
    },
    /// List built-in templates or add a custom template JSON definition.
    Template {
        #[command(subcommand)]
        action: Templates,
    },
    /// Extract and retain documents locally for review. Never auto-approves.
    Process {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        #[arg(long)]
        profile: String,
        #[arg(long, default_value = "e_archive")]
        template: String,
        #[arg(long, value_enum, default_value = "e-archive")]
        kind: DocumentKind,
        #[arg(long)]
        endpoint: Option<String>,
        #[arg(long)]
        model: Option<String>,
        /// Authorize sending document contents to the specified external endpoint.
        #[arg(long)]
        allow_remote_document_transfer: bool,
        /// Import deterministic results or blank review records without inference.
        #[arg(long)]
        no_model: bool,
        #[arg(long)]
        no_merge_taxes: bool,
    },
    /// List persisted documents as JSON.
    List {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        approved: bool,
    },
    /// Show the full review record and validation issues.
    Show {
        id: String,
    },
    /// Save human corrections; requires the revision returned by show.
    Review {
        id: String,
        #[arg(long)]
        revision: u64,
        #[arg(long = "set")]
        fields: Vec<String>,
        #[arg(long)]
        taxes: Option<PathBuf>,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        merge_taxes: Option<bool>,
    },
    /// Approve one reviewed document, optionally acknowledging specific missing fields.
    Approve {
        id: String,
        #[arg(long)]
        revision: u64,
        #[arg(long = "acknowledge-missing")]
        acknowledge: Vec<String>,
    },
    Reopen {
        id: String,
        #[arg(long)]
        revision: u64,
    },
    /// Permanently delete the locally retained document and work record.
    Delete {
        id: String,
        #[arg(long)]
        revision: u64,
    },
    Export {
        #[arg(long)]
        output: PathBuf,
        #[arg(long, value_enum)]
        format: Option<ExportFormat>,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long = "id")]
        ids: Vec<String>,
    },
    Settings {
        #[command(subcommand)]
        action: SettingAction,
    },
    Backup {
        destination: PathBuf,
    },
    /// Local extraction accuracy gate. Only run after dataset readiness is confirmed.
    Benchmark {
        #[arg(long, env = "FATROCU_BENCHMARK_DATA")]
        dataset: PathBuf,
        #[arg(long)]
        report: PathBuf,
        #[arg(long)]
        commit: String,
    },
    /// Check published stable releases; installation requires digest and publisher verification.
    Update {
        #[arg(long)]
        install: bool,
    },
    /// Check the configured HTTP/OpenAI-compatible model connection without documents.
    ConnectionCheck,
    /// Print a blank extraction schema for integrations and ground-truth preparation.
    Schema {
        #[arg(long, value_enum, default_value = "e-archive")]
        kind: DocumentKind,
    },
}
#[derive(Subcommand)]
enum Profiles {
    List,
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        tax_id: String,
    },
    Rename {
        id: String,
        #[arg(long)]
        name: String,
    },
    Delete {
        id: String,
    },
}
#[derive(Subcommand)]
enum Templates {
    List,
    Add { file: PathBuf },
}
#[derive(Subcommand)]
enum SettingAction {
    Show,
    Set {
        #[arg(long)]
        telemetry: Option<bool>,
        #[arg(long)]
        endpoint: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        api_key_env: Option<String>,
        #[arg(long, value_enum)]
        protocol: Option<fatrocu_core::inference::Protocol>,
        #[arg(long)]
        default_export: Option<String>,
    },
}
fn print(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
fn startup(root: PathBuf) {
    if std::env::var_os("FATROCU_NO_UPDATE_CHECK").is_some() {
        return;
    }
    std::thread::spawn(move || {
        if let Ok(release) = update::check("fatrocu-cli", &root.join("updates"), false)
            && update::newer(&release, env!("CARGO_PKG_VERSION")).unwrap_or(false)
        {
            eprintln!(
                "Fatrocu CLI {} is available. Run fatrocu-cli update for release notes.",
                release.tag_name
            );
        }
    });
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let cli = Cli::parse();
    let root = cli
        .data_dir
        .map(Ok)
        .unwrap_or_else(storage::default_data_dir)?;
    startup(root.clone());
    let mut store = Store::open(root)?;
    match cli.command {
        Commands::Profile { action } => match action {
            Profiles::List => print(&store.profiles()?)?,
            Profiles::Add { name, tax_id } => {
                let p = Profile::new(name, tax_id)?;
                store.save_profile(&p)?;
                print(&p)?;
            }
            Profiles::Rename { id, name } => {
                let mut p = store.profile(&id)?;
                p.legal_name = name;
                store.save_profile(&p)?;
                print(&p)?;
            }
            Profiles::Delete { id } => store.delete_profile(&id)?,
        },
        Commands::Template { action } => match action {
            Templates::List => print(&store.templates()?)?,
            Templates::Add { file } => {
                let t: Template = serde_json::from_slice(&std::fs::read(file)?)?;
                store.save_template(&t)?;
                print(&t)?;
            }
        },
        Commands::Process {
            files,
            profile,
            template,
            kind,
            endpoint,
            model,
            allow_remote_document_transfer,
            no_model,
            no_merge_taxes,
        } => {
            let profile = store.profile(&profile)?;
            let template = store.template(&template)?;
            let mut connection = store.settings()?.endpoint;
            if let Some(url) = endpoint {
                connection.url = url;
            }
            if let Some(model) = model {
                connection.model = model;
            }
            if connection.is_remote() && !no_model {
                eprintln!("{}", connection.disclosure());
                connection.authorize(allow_remote_document_transfer)?;
            }
            let mut result = vec![];
            let mut failed = false;
            for file in files {
                match extraction::extract(
                    &file,
                    &profile,
                    &template,
                    kind,
                    &connection,
                    allow_remote_document_transfer,
                    !no_model,
                )
                .and_then(|mut d| {
                    d.merge_taxes = !no_merge_taxes;
                    store.import(&file, d)
                }) {
                    Ok(d) => result.push(serde_json::json!({"document":d,"success":true})),
                    Err(err) => {
                        failed = true;
                        result
                            .push(serde_json::json!({"success":false,"error":format!("{err:#}")}));
                    }
                }
            }
            print(&result)?;
            ensure!(!failed, "One or more documents could not be imported");
        }
        Commands::List { profile, approved } => print(
            &store
                .documents()?
                .into_iter()
                .filter(|d| {
                    profile.as_ref().is_none_or(|p| *p == d.profile_id)
                        && (!approved || d.status == ReviewStatus::Approved)
                })
                .collect::<Vec<_>>(),
        )?,
        Commands::Show { id } => {
            let d = store.document(&id)?;
            print(
                &serde_json::json!({"issues":d.issues(),"source":store.source(&id)?,"document":d}),
            )?;
        }
        Commands::Review {
            id,
            revision,
            fields,
            taxes,
            input,
            merge_taxes,
        } => {
            let mut d = if let Some(file) = input {
                serde_json::from_slice::<Document>(&std::fs::read(file)?)?
            } else {
                store.document(&id)?
            };
            ensure!(
                d.id == id && d.revision == revision,
                "Review input identity/revision mismatch"
            );
            for pair in fields {
                let (key, value) = pair
                    .split_once('=')
                    .context("Use --set field=value; an empty value means null")?;
                ensure!(d.fields.contains_key(key), "Unknown template field: {key}");
                d.set(key, Some(value.into()));
            }
            if let Some(file) = taxes {
                d.taxes = serde_json::from_slice(&std::fs::read(file)?)?;
            }
            if let Some(merge) = merge_taxes {
                d.merge_taxes = merge;
            }
            store.review(&mut d)?;
            print(&d)?;
        }
        Commands::Approve {
            id,
            revision,
            acknowledge,
        } => print(&store.approve(&id, revision, &acknowledge)?)?,
        Commands::Reopen { id, revision } => print(&store.reopen(&id, revision)?)?,
        Commands::Delete { id, revision } => store.delete(&id, revision)?,
        Commands::Export {
            output,
            format,
            profile,
            ids,
        } => {
            let docs = store
                .documents()?
                .into_iter()
                .filter(|d| {
                    d.status == ReviewStatus::Approved
                        && profile.as_ref().is_none_or(|p| *p == d.profile_id)
                        && (ids.is_empty() || ids.contains(&d.id))
                })
                .collect::<Vec<_>>();
            ensure!(
                ids.iter().all(|id| docs.iter().any(|d| d.id == *id)),
                "Some selected IDs are missing, unapproved, or belong to another profile"
            );
            let format = format.unwrap_or(store.settings()?.default_export.parse()?);
            export::export(&docs, format, &output)?;
            print(&serde_json::json!({"exported":docs.len(),"output":output}))?;
        }
        Commands::Settings { action } => match action {
            SettingAction::Show => print(
                &serde_json::json!({"settings":store.settings()?,"telemetry_package":"Deferred pending contents and deletion policy decision; no collection or purge is implemented"}),
            )?,
            SettingAction::Set {
                telemetry,
                endpoint,
                model,
                api_key_env,
                protocol,
                default_export,
            } => {
                let mut s: Settings = store.settings()?;
                if let Some(v) = telemetry {
                    s.telemetry_enabled = v;
                }
                if let Some(v) = endpoint {
                    s.endpoint.url = v;
                }
                if let Some(v) = model {
                    s.endpoint.model = v;
                }
                if let Some(v) = api_key_env {
                    s.endpoint.api_key_env = if v.is_empty() { None } else { Some(v) };
                }
                if let Some(v) = protocol {
                    s.endpoint.protocol = v;
                }
                if let Some(v) = default_export {
                    s.default_export = v;
                }
                store.save_settings(&s)?;
                print(&s)?;
            }
        },
        Commands::Backup { destination } => store.backup(&destination)?,
        Commands::ConnectionCheck => {
            store.settings()?.endpoint.health()?;
            print(&serde_json::json!({"connected":true}))?;
        }
        Commands::Benchmark {
            dataset,
            report,
            commit,
        } => {
            ensure!(
                std::env::var("FATROCU_BENCHMARK_ENABLED").as_deref() == Ok("1"),
                "Benchmark is on hold. Confirm dataset readiness and update the agent instructions before setting FATROCU_BENCHMARK_ENABLED=1"
            );
            let endpoint: Endpoint = store.settings()?.endpoint;
            fatrocu_core::storage::outside_repository(&report)?;
            let result = benchmark::run(&dataset, &endpoint, commit)?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(report)?;
            serde_json::to_writer_pretty(&mut file, &result)?;
            print(&result)?;
            ensure!(result.passed, "Extraction benchmark thresholds failed");
        }
        Commands::Update { install } => {
            let release = update::check("fatrocu-cli", &store.root.join("updates"), true)?;
            let available = update::newer(&release, env!("CARGO_PKG_VERSION"))?;
            print(&serde_json::json!({"available":available,"release":release}))?;
            if install && available {
                let staged = update::stage(&release, false)?;
                let executable = std::env::current_exe()?;
                let directory = executable
                    .parent()
                    .context("Missing installation directory")?;
                #[cfg(windows)]
                {
                    update::launch_helper(&staged, directory, None)?;
                }
                #[cfg(not(windows))]
                {
                    update::install(&staged, directory)?;
                }
            }
        }
        Commands::Schema { kind } => {
            let profile = Profile {
                id: String::new(),
                legal_name: String::new(),
                tax_id: String::new(),
            };
            let d = Document::blank(&profile, &Template::builtin(kind), kind);
            print(&BTreeMap::from([
                ("fields", serde_json::to_value(d.fields)?),
                (
                    "taxes",
                    serde_json::json!([TaxRow {
                        tax_type: Field::new(None),
                        rate: Field::new(None),
                        base: Field::new(None),
                        amount: Field::new(None)
                    }]),
                ),
            ]))?;
        }
    }
    Ok(())
}
