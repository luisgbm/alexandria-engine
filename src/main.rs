use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[cfg(unix)]
    /// Incrementally ingest staging-only structured Nginx logs.
    AnalyticsIngest {
        #[arg(long)]
        site_root: PathBuf,
        #[arg(long)]
        logs: PathBuf,
        #[arg(long)]
        state: PathBuf,
    },
    #[cfg(unix)]
    /// Run the private, loopback-only staging administration service.
    ServeAdmin {
        #[arg(long)]
        content: PathBuf,
        #[arg(long)]
        state: PathBuf,
        #[arg(long, default_value = "127.0.0.1:3001")]
        listen: String,
        #[arg(long, default_value = "http://localhost:8781")]
        origin: String,
        #[arg(long, default_value = "localhost")]
        rp_id: String,
    },
    #[cfg(unix)]
    /// Initialize staging release history without changing the active release.
    InitStagingPublisher {
        #[arg(long)]
        content: PathBuf,
        #[arg(long)]
        state: PathBuf,
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        public_url: String,
    },
    #[cfg(unix)]
    /// Publish one saved revision on staging from the root console.
    PublishStaging {
        #[arg(long)]
        content: PathBuf,
        #[arg(long)]
        state: PathBuf,
        #[arg(long)]
        id: String,
        #[arg(long)]
        revision: String,
        #[arg(long)]
        expected_active: String,
    },
    #[cfg(unix)]
    /// Activate an existing verified staging release from the root console.
    RollbackStaging {
        #[arg(long)]
        state: PathBuf,
        #[arg(long)]
        release: String,
        #[arg(long)]
        expected_active: String,
    },
    #[cfg(unix)]
    /// Root-console-only initial enrollment and offline recovery material.
    AdminInit {
        #[arg(long)]
        state: PathBuf,
        #[arg(long)]
        secrets: PathBuf,
        #[arg(long, default_value = "http://localhost:8781")]
        origin: String,
        #[arg(long, default_value = "localhost")]
        rp_id: String,
    },
    #[cfg(unix)]
    /// Root-console-only recovery; consumes and rotates the offline secret.
    AdminRecover {
        #[arg(long)]
        state: PathBuf,
        #[arg(long)]
        recovery_file: PathBuf,
        #[arg(long)]
        new_secrets: PathBuf,
    },
    /// Import a WordPress tar.gz backup without running PHP or SQL.
    ImportWordpress {
        #[arg(long)]
        archive: PathBuf,
        #[arg(long)]
        rendered_html: PathBuf,
        #[arg(long)]
        inventory: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Build a complete static release into a new directory.
    Build {
        #[arg(long)]
        content: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

fn main() -> Result<()> {
    match Args::parse().command {
        #[cfg(unix)]
        Command::AnalyticsIngest {
            site_root,
            logs,
            state,
        } => {
            let count = alexandria_engine::analytics::ingest(&site_root, &logs, &state)?;
            println!(
                "Analytics ingestion completed: {count} new counted page views; raw request data is not printed."
            );
        }
        #[cfg(unix)]
        Command::InitStagingPublisher {
            content,
            state,
            root,
            public_url,
        } => alexandria_engine::admin::initialize_publishing(&content, &state, &root, &public_url)?,
        #[cfg(unix)]
        Command::PublishStaging {
            content,
            state,
            id,
            revision,
            expected_active,
        } => alexandria_engine::admin::console_publish(
            &content,
            &state,
            &id,
            &revision,
            &expected_active,
        )?,
        #[cfg(unix)]
        Command::RollbackStaging {
            state,
            release,
            expected_active,
        } => alexandria_engine::admin::console_rollback(&state, &release, &expected_active)?,
        #[cfg(unix)]
        Command::ServeAdmin {
            content,
            state,
            listen,
            origin,
            rp_id,
        } => tokio::runtime::Runtime::new()?.block_on(alexandria_engine::admin::serve(
            content, state, listen, origin, rp_id,
        ))?,
        #[cfg(unix)]
        Command::AdminInit {
            state,
            secrets,
            origin,
            rp_id,
        } => alexandria_engine::admin::initialize(&state, &secrets, &origin, &rp_id)?,
        #[cfg(unix)]
        Command::AdminRecover {
            state,
            recovery_file,
            new_secrets,
        } => alexandria_engine::admin::recover(&state, &recovery_file, &new_secrets)?,
        Command::ImportWordpress {
            archive,
            rendered_html,
            inventory,
            output,
        } => alexandria_engine::import::run(&archive, &rendered_html, &inventory, &output)?,
        Command::Build { content, output } => alexandria_engine::build::run(&content, &output)?,
    }
    Ok(())
}
