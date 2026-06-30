use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use termfx::core::media::AssetKind;
use termfx::mcp::server::run_stdio_server;
use termfx::project::Project;
use termfx::render::ffmpeg::{RenderSettings, build_ffmpeg_command};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(name = "termfx")]
#[command(about = "Terminal-native video editor with FFmpeg and MCP integration.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    New {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "termfx.project.json")]
        project: PathBuf,
    },
    AddMedia {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = CliAssetKind::Video)]
        kind: CliAssetKind,
        #[arg(long)]
        name: Option<String>,
    },
    AddClip {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        media_id: Uuid,
        #[arg(long, default_value_t = 0)]
        track: usize,
        #[arg(long, default_value_t = 0.0)]
        start_seconds: f64,
        #[arg(long)]
        duration_seconds: f64,
    },
    Tui {
        #[arg(long, default_value = "termfx.project.json")]
        project: PathBuf,
    },
    Mcp {
        #[arg(long, default_value = "termfx.project.json")]
        project: PathBuf,
    },
    Render {
        #[arg(long, default_value = "termfx.project.json")]
        project: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Clone, Debug, ValueEnum)]
enum CliAssetKind {
    Video,
    Audio,
    Image,
}

impl From<CliAssetKind> for AssetKind {
    fn from(value: CliAssetKind) -> Self {
        match value {
            CliAssetKind::Video => AssetKind::Video,
            CliAssetKind::Audio => AssetKind::Audio,
            CliAssetKind::Image => AssetKind::Image,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::New { name, project } => {
            let created = Project::new(name, std::env::current_dir()?);
            created.save(&project)?;
            println!("Created {}", project.display());
        }
        Command::AddMedia {
            project,
            path,
            kind,
            name,
        } => {
            let mut loaded = Project::load(&project)?;
            let asset = loaded.add_media(path, kind.into(), name);
            loaded.save(&project)?;
            println!("Added media {} ({})", asset.name, asset.id);
        }
        Command::AddClip {
            project,
            media_id,
            track,
            start_seconds,
            duration_seconds,
        } => {
            let mut loaded = Project::load(&project)?;
            let start_frame = loaded.timeline.fps.frames_from_seconds(start_seconds);
            let duration_frames = loaded.timeline.fps.frames_from_seconds(duration_seconds);
            let clip_id = loaded.add_media_clip(media_id, track, start_frame, duration_frames)?;
            loaded.save(&project)?;
            println!("Added clip {}", clip_id);
        }
        Command::Tui { project } => {
            let loaded = Project::load(&project)?;
            termfx::tui::app::run(loaded)?;
        }
        Command::Mcp { project } => {
            run_stdio_server(project).await?;
        }
        Command::Render {
            project,
            output,
            dry_run,
        } => {
            let loaded = Project::load(&project)?;
            let command = build_ffmpeg_command(
                &loaded,
                &output,
                RenderSettings::from_timeline(&loaded.timeline),
            )?;
            if dry_run {
                println!("{}", command.display_shell());
            } else {
                command.spawn_and_wait()?;
            }
        }
    }

    Ok(())
}
