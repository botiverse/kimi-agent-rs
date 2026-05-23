use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use kaos::KaosPath;
use tracing::info;

use crate::metadata::load_metadata;
use crate::session::Session;

#[derive(Args, Debug)]
#[command(about = "Export session data as a ZIP archive.")]
pub struct ExportArgs {
    #[arg(help = "Session ID to export. Defaults to the previous session.")]
    pub session_id: Option<String>,

    #[arg(
        long = "output",
        short = 'o',
        value_name = "PATH",
        help = "Output ZIP file path. Default: session-{id}.zip in current directory."
    )]
    pub output: Option<PathBuf>,
}

pub async fn run_export_command(args: ExportArgs) -> Result<()> {
    let work_dir = KaosPath::cwd();

    let session_dir = if let Some(session_id) = args.session_id {
        let session = Session::find(work_dir.clone(), &session_id).await;
        match session {
            Some(s) => s.dir(),
            None => {
                anyhow::bail!("Session '{}' not found.", session_id);
            }
        }
    } else {
        let metadata = load_metadata().await;
        let work_dir_str = work_dir.to_string();
        let session_id = metadata
            .work_dirs
            .iter()
            .find(|meta| meta.path == work_dir_str)
            .and_then(|meta| meta.last_session_id.clone());

        match session_id {
            Some(id) => {
                let session = Session::find(work_dir.clone(), &id).await;
                match session {
                    Some(s) => s.dir(),
                    None => {
                        anyhow::bail!("No previous session found for the working directory.");
                    }
                }
            }
            None => {
                anyhow::bail!("No previous session found for the working directory.");
            }
        }
    };

    if !session_dir.exists() {
        anyhow::bail!("Session directory does not exist.");
    }

    let session_id = session_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_string();

    let output_path = args.output.unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(format!("session-{}.zip", session_id))
    });

    info!(
        "Exporting session {} to {}",
        session_id,
        output_path.display()
    );

    let file = std::fs::File::create(&output_path)
        .with_context(|| format!("Failed to create output file: {}", output_path.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    fn add_dir_to_zip(
        zip: &mut zip::ZipWriter<std::fs::File>,
        base: &std::path::Path,
        dir: &std::path::Path,
        options: zip::write::SimpleFileOptions,
    ) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(base)?;
            if path.is_file() {
                zip.start_file_from_path(relative, options)?;
                let contents = std::fs::read(&path)?;
                zip.write_all(&contents)?;
            } else if path.is_dir() {
                add_dir_to_zip(zip, base, &path, options)?;
            }
        }
        Ok(())
    }

    add_dir_to_zip(&mut zip, &session_dir, &session_dir, options)?;

    zip.finish()?;

    println!("{}", output_path.display());

    Ok(())
}
