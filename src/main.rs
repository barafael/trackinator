use anyhow::Context;
use clap::Parser;
use laby::{html, iter, render};
use lychee_lib::Response;
use serde::{Deserialize, Serialize};
use std::{fs::File, io::BufReader, path::PathBuf, process};

#[derive(Debug, Parser)]
pub enum Action {
    /// Generate from `manifest` the HTML `output`
    Generate {
        /// The JSON input file
        #[arg(long, default_value = "tracks.json")]
        manifest: PathBuf,

        /// The file to write the output to
        #[arg(long, default_value = "index.html")]
        output: PathBuf,
    },
    /// Add a track with `name` and `path` to `manifest`
    Add {
        /// The `manifest` to modify
        #[arg(long, default_value = "tracks.json")]
        manifest: PathBuf,

        /// The `name` of the new song
        #[arg(long)]
        name: String,

        /// The `path` of the new song
        #[arg(long)]
        path: PathBuf,
    },
    /// Check an existing manifest:
    /// * Check each linked file is actually reachable
    Check {
        /// The `manifest` to check
        #[arg(long, default_value = "tracks.json")]
        manifest: PathBuf,
    },
    /// Format a `manifest`
    Format {
        /// The `manifest` to format
        #[arg(long, default_value = "tracks.json")]
        manifest: PathBuf,
    },
    /// Generate a template manifest with default values
    Template {
        /// The `manifest` path
        #[arg(long, default_value = "tracks.json")]
        manifest: PathBuf,
    },
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Arguments {
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Song {
    name: String,
    path: PathBuf,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Manifest {
    title: String,
    prefix: String,
    songs: Vec<Song>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Arguments::parse();
    match args.action {
        Action::Generate { manifest, output } => {
            let reader = BufReader::new(File::open(manifest).context("Failed to open manifest")?);
            let manifest: Manifest =
                serde_json::from_reader(reader).context("Failed to read manifest")?;

            let audio_tags = iter!(manifest.songs.iter().map(|s| laby::div!(
                laby::h3!(s.name.clone()),
                laby::audio!(
                    class = "track",
                    controls = "controls",
                    source!(src = song_url(&manifest.prefix, s.path.to_str().unwrap_or_default()))
                )
            )));

            let n = html!(
                head!(title!(manifest.title),),
                body!(class = "dark", audio_tags),
            );

            let result = render!(n);
            std::fs::write(output, result)?;
        }
        Action::Check { manifest } => {
            let reader = BufReader::new(File::open(manifest).context("Failed to open manifest")?);
            let manifest: Manifest =
                serde_json::from_reader(reader).context("Failed to read manifest")?;
            let mut handles = Vec::new();
            for song in manifest.songs {
                let url = song_url(&manifest.prefix, song.path.to_str().unwrap_or_default());
                let handle = tokio::spawn({
                    println!("Checking {url}");
                    lychee_lib::check(url)
                });
                handles.push(handle);
            }
            let responses = futures::future::try_join_all(handles)
                .await
                .context("Failed to join the check tasks")?
                .into_iter()
                .collect::<Result<Vec<Response>, _>>()
                .context("Resource unreachable")?;
            let mut error = false;
            for response in responses {
                if !response.status().is_success() {
                    error = true;
                    eprintln!("not reachable {}", response.0)
                }
            }
            if error {
                process::exit(1);
            }
        }
        Action::Add {
            manifest: file,
            name,
            path,
        } => {
            let reader = BufReader::new(File::open(&file).context("Failed to open manifest")?);
            let mut manifest: Manifest =
                serde_json::from_reader(reader).context("Failed to read manifest")?;
            let new_song = Song { name, path };
            manifest.songs.push(new_song);
            let manifest =
                serde_json::to_string_pretty(&manifest).context("Failed to serialize manifest")?;
            std::fs::write(&file, manifest).context("Failed to write manifest")?;
        }
        Action::Format { manifest: file } => {
            let reader = BufReader::new(File::open(&file).context("Failed to open manifest")?);
            let manifest: Manifest =
                serde_json::from_reader(reader).context("Failed to read manifest")?;
            let manifest =
                serde_json::to_string_pretty(&manifest).context("Failed to serialize manifest")?;
            std::fs::write(&file, manifest).context("Failed to write manifest")?;
        }
        Action::Template { manifest } => {
            let empty = Manifest::default();
            let template = serde_json::to_string_pretty(&empty)
                .context("Failed to serialize default manifest template")?;
            std::fs::write(manifest, template).context("Failed to write template manifest")?;
        }
    }

    Ok(())
}

fn song_url(prefix: &str, path: &str) -> String {
    format!("{}{}", prefix, path)
}
