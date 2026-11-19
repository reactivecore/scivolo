//! Builds the library image of the standard libraries if needed and prints its file's path.

fn main() -> Result<(), String> {
    let image = libs::image(&libs::jars()?).map_err(|e| e.to_string())?;
    println!("{}", libs::cache_dir().join(format!("libs-{:016x}.img", image.key)).display());
    Ok(())
}
