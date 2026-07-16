use color_eyre::Result;
use piston_mc::manifest_v2::ManifestV2;
use simple_download_utility::DownloadProgress;
use std::env::temp_dir;
use std::path::{Path, PathBuf};
use std::thread;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let (temp, jar_path) = download_jar().await?;
    let recipes_dir = extract_jar(&temp, &jar_path)?;

    println!("Copying recipes file to target");
    let target_recipes = PathBuf::from(env!("TEST_RECIPE_DIRECTORY"));
    std::fs::create_dir_all(&target_recipes)?;
    for recipe in std::fs::read_dir(&recipes_dir)? {
        let recipe = recipe?;
        let path = recipe.path();
        if path.is_dir() {
            continue;
        }
        let filename = path
            .file_name()
            .expect("failed to get filename")
            .to_string_lossy()
            .to_string();
        let target = target_recipes.join(filename);
        println!("Copying {path:?} to {target:?}");
        std::fs::copy(path, target)?;
    }
    cleanup(&temp)?;
    println!("Done!");
    Ok(())
}

async fn download_jar() -> Result<(PathBuf, PathBuf)> {
    let manifest = ManifestV2::fetch().await.expect("Manifest V2 fetch error");
    let version = manifest
        .releases()
        .first()
        .expect("No latest release")
        .manifest()
        .await
        .expect("No latest release");
    let temp = temp_dir().join("ccmk4_extract");
    let jar_file = temp.join("client.jar");
    if jar_file.exists() {
        std::fs::remove_file(&jar_file)?;
    }
    println!("Downloading {} to {:?}", version.id, jar_file);
    let (tx, mut rx) = tokio::sync::mpsc::channel::<DownloadProgress>(32);

    thread::spawn(async move || {
        while let Some(data) = rx.recv().await {
            let percentage = (data.bytes_downloaded as f32 / data.bytes_to_download as f32) * 100.0;
            println!(
                "{:0.0}% - {:0.0}MBps",
                percentage,
                data.bytes_per_second as f32 / 1024.0 / 1024.0
            );
        }
    });

    version
        .download_client(&jar_file, true, Some(tx))
        .await
        .expect("Download client error");
    Ok((temp, jar_file))
}

fn extract_jar(temp: &Path, jar_file: &PathBuf) -> Result<PathBuf> {
    let archive_extract_dir = temp.join("extract");
    println!("Extracting {}", temp.display());
    let file = std::fs::File::open(jar_file)?;
    let mut archive = zip::ZipArchive::new(file)?;
    archive.extract(&archive_extract_dir)?;
    println!(
        "Extracted {} from {}",
        jar_file.display(),
        archive_extract_dir.display()
    );
    Ok(archive_extract_dir
        .join("data")
        .join("minecraft")
        .join("recipe"))
}

fn cleanup(path: &PathBuf) -> Result<()> {
    println!("Cleaning up {}", path.display());
    std::fs::remove_dir_all(path)?;
    Ok(())
}
